use super::{Action, LocalRequest, Manager, Prepared, Slot, State, key, stop};
use crate::{
    config::{AuthorizedServer, Definition},
    error::{Error, ErrorKind},
    install::{
        preparation::{self, Context},
        records,
        target::Target,
    },
    mcp::{Client, Discovery, HttpOptions, Limits, Operation},
    ownership::{Installation, OperationState, Registration, Verification},
};
use serde_json::Value;
use std::{sync::Arc, time::Instant};
use tokio::sync::{MutexGuard, OwnedSemaphorePermit};

impl Manager {
    pub(super) async fn install(
        &self,
        request: LocalRequest,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let Action::Install(options): &Action = &request.action else {
            return Err(crate::install::invalid());
        };
        let target: Arc<Target> = Arc::new(
            Target::lock(
                &self.paths,
                &request.cwd,
                &request.server,
                options,
                operation,
            )
            .await?,
        );
        let key: String = key(&target.identity)?;
        let slot: Arc<Slot> = operation.run(self.slot(&key)).await?;
        let _queue: OwnedSemaphorePermit = slot
            .queue
            .clone()
            .try_acquire_owned()
            .map_err(|_| super::busy())?;
        let mut state: MutexGuard<'_, State> =
            operation.run(async { Ok(slot.state.lock().await) }).await?;
        if state.client.is_some() {
            return Err(Error::new(
                ErrorKind::Configuration,
                "an owned process still occupies the installation target; stop it before retrying",
            ));
        }
        self.require_clean(&key)?;
        let mut installation: Installation = records::begin(
            target.identity.source.clone(),
            request.server.clone(),
            &options.server.definition,
        )?;
        installation.definition = Some(options.server.clone());
        installation.configuration_digest = Some(target.identity.configuration_digest.clone());
        records::save(
            &self.paths,
            &installation,
            OperationState::Prepared,
            operation,
        )
        .await?;
        let result: Result<(), Error> = self
            .stage(&request, &target, &mut installation, &mut state, operation)
            .await;
        if let Err(error) = result {
            return Err(self
                .rollback_install(&target, &mut installation, &mut state, error)
                .await);
        }
        self.commit_install(&target, &mut installation, &mut state, operation)
            .await?;
        crate::install::report::report(&target, &installation, options)
    }

    async fn stage(
        &self,
        request: &LocalRequest,
        target: &Arc<Target>,
        installation: &mut Installation,
        state: &mut State,
        operation: &Operation,
    ) -> Result<(), Error> {
        preparation::resolve_executable(installation, &request.environment)?;
        records::save(
            &self.paths,
            installation,
            OperationState::Applying,
            operation,
        )
        .await?;
        target.check()?;
        installation.origin = preparation::prepare(&Context {
            paths: &self.paths,
            target,
            environment: &request.environment,
            installation,
            operation,
        })
        .await?;
        self.require_clean(&crate::config::digest(
            installation.id().as_str().as_bytes(),
        ))?;
        records::save(
            &self.paths,
            installation,
            OperationState::Applying,
            operation,
        )
        .await?;
        target.check()?;
        let Action::Install(options): &Action = &request.action else {
            return Err(crate::install::invalid());
        };
        installation.verification = Verification::Unverified;
        if !options.skip_verify {
            match &options.server.definition {
                Definition::Local { .. } => {
                    self.verify_local(request, target, installation, state, operation)
                        .await?
                }
                Definition::Remote { .. } => verify_remote(request, target, operation).await?,
            }
            installation.verification = Verification::Verified;
        }
        target.check()?;
        operation.remaining()?;
        Ok(())
    }

    async fn verify_local(
        &self,
        request: &LocalRequest,
        target: &Target,
        installation: &Installation,
        state: &mut State,
        operation: &Operation,
    ) -> Result<(), Error> {
        let server: AuthorizedServer<'_> = target.effective.authorize(&request.server)?;
        let key: String = key(&target.identity)?;
        let prepared: Prepared = Prepared::for_installation(
            &server,
            &self.paths,
            &request.environment,
            &key,
            operation,
            installation,
        )
        .await?;
        let lease: std::fs::File = self.lease(&key, operation).await?;
        target.check()?;
        operation.remaining()?;
        drop(lease);
        let client: Client = Client::stdio(
            prepared.options,
            Limits {
                in_flight: 16,
                ..Limits::default()
            },
            operation,
        )
        .await?;
        state.client = Some(client);
        let discovery: Discovery = state
            .client
            .as_ref()
            .ok_or_else(crate::install::invalid)?
            .discover_tools(operation)
            .await?;
        discovery.require_complete()?;
        target.check()?;
        state.revision = prepared.revision;
        state.configuration_digest = prepared.configuration_digest;
        state.installation = Some(prepared.installation);
        state.observed = Some(Instant::now());
        Ok(())
    }

    async fn rollback_install(
        &self,
        target: &Target,
        installation: &mut Installation,
        state: &mut State,
        error: Error,
    ) -> Error {
        stop(state).await;
        let preparation_key: String = crate::config::digest(installation.id().as_str().as_bytes());
        let clean = key(&target.identity).is_ok_and(|key| self.require_clean(&key).is_ok())
            && self.require_clean(&preparation_key).is_ok();
        installation.registration = Registration::Unregistered;
        let saved = records::save(
            &self.paths,
            installation,
            OperationState::RetryRequired,
            &cleanup_operation(),
        )
        .await
        .is_ok();
        if !clean || !saved {
            return incomplete(installation);
        }
        recovery(error, installation)
    }

    async fn commit_install(
        &self,
        target: &Target,
        installation: &mut Installation,
        state: &mut State,
        operation: &Operation,
    ) -> Result<(), Error> {
        installation.registration = Registration::Registered;
        if let Err(error) = records::save(
            &self.paths,
            installation,
            OperationState::Applying,
            operation,
        )
        .await
        {
            return Err(self
                .rollback_install(target, installation, state, error)
                .await);
        }
        if let Err(error) = target.commit(operation) {
            return Err(self
                .rollback_install(target, installation, state, error)
                .await);
        }
        if records::save(
            &self.paths,
            installation,
            OperationState::Committed,
            &cleanup_operation(),
        )
        .await
        .is_err()
        {
            stop(state).await;
            return Err(incomplete(installation));
        }
        Ok(())
    }
}

async fn verify_remote(
    request: &LocalRequest,
    target: &Arc<Target>,
    operation: &Operation,
) -> Result<(), Error> {
    let options: HttpOptions = crate::auth::remote::installation_options(
        target.clone(),
        &request.cwd,
        &request.environment,
        operation,
    )
    .await?;
    let client: Client = Client::http(options, Limits::default(), operation).await?;
    let result: Result<Discovery, Error> = client.discover_tools(operation).await;
    client.shutdown().await;
    result?.require_complete()
}

fn recovery(error: Error, installation: &Installation) -> Error {
    let error: Error = if error.kind == ErrorKind::Authentication
        && matches!(installation.origin, crate::ownership::Origin::Remote { .. })
    {
        Error::new(
            ErrorKind::Authentication,
            "authentication required before registration; for OAuth configure authentication as oauth, repeat install with --skip-verify, then run auth login <server> and tools <server>; otherwise provide required secret references and retry install; for project scope run trust after registration, and ensure the intended scope is not shadowed",
        )
    } else {
        error
    };
    Error { kind: error.kind, message: format!("{}; installation {} rollback recorded; shared runtime storage and user paths preserved", error.message, installation.id().as_str()).into() }
}
fn incomplete(installation: &Installation) -> Error {
    Error {
        kind: ErrorKind::PartialFailure,
        message: format!("installation {} incomplete; possible owned process/container residuals; inspect this installation's ownership.json operation and recorded process leases before retrying; shared resources preserved", installation.id().as_str()).into(),
    }
}

fn cleanup_operation() -> Operation {
    Operation::new(
        crate::deadline::Deadline::new(std::time::Duration::from_secs(5)),
        crate::mcp::CancellationToken::new(),
    )
}
