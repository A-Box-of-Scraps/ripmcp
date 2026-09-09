mod change;

use super::{SecureStore, SystemStore};
use crate::{
    cli::AuthConfigure,
    config::{
        AuthorizedServer, Configuration, Effective, MutationReport, Server, Source, WriteScope,
        schema::SecretReference,
    },
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
    storage::Paths,
    trust::secrets::Secret,
};
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) async fn execute(args: AuthConfigure, timeout: Option<u64>) -> Result<(), Error> {
    let signal: SignalCancellation = SignalCancellation::install()?;
    let started: Instant = Instant::now();
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let effective: Effective = Effective::load(&paths, &cwd)?;
    let operation: Operation = Operation::new(
        Deadline::new(
            Duration::from_secs(timeout.unwrap_or(effective.timeouts().login_seconds.get()))
                .saturating_sub(started.elapsed()),
        ),
        signal.token(),
    );
    let server: AuthorizedServer<'_> = effective.authorize(&args.server)?;
    if matches!(server.identity().source, Source::Project { .. }) != args.scope.project {
        return Err(Error::new(
            ErrorKind::Configuration,
            "authentication target is not in the selected scope or is shadowed; select --project or run outside the overriding project",
        ));
    }
    let original: Server = server.server().clone();
    let placeholder: SecretReference = SecretReference::Stored("0".repeat(64));
    let mut updated: Server = change::server(&args, &original, &placeholder)?;
    let _maintenance: crate::storage::Maintenance =
        crate::storage::Maintenance::acquire(&paths, false, &operation).await?;
    if needs_secret(&args) {
        operation.run(SystemStore.available()).await?;
        eprintln!(
            "Configure credentials for server {:?}. No tool will be invoked.",
            args.server
        );
        let secret: Secret = super::prompt::read(&operation).await?;
        if args.source.bearer && !super::token::bearer(secret.expose()) {
            return Err(Error::new(
                ErrorKind::Authentication,
                "invalid bearer token; supply only the raw token, without the Bearer prefix",
            ));
        }
        let reference: SecretReference = save(&SystemStore, &secret, &operation).await?;
        updated = change::server(&args, &original, &reference)?;
    }
    let current: Effective = Effective::load(&paths, &cwd)?;
    if current.authorize(&args.server)?.identity() != server.identity() {
        return Err(Error::new(
            ErrorKind::Configuration,
            "configuration changed during credential setup; retry",
        ));
    }
    let scope: WriteScope = (&args.scope).into();
    let ((), report): ((), MutationReport) = effective.mutate_with_deadline(
        &paths,
        scope,
        &args.server,
        &Deadline::new(operation.remaining()?),
        |config: &mut Configuration| {
            let target: &mut Server = config
                .servers
                .get_mut(&args.server)
                .ok_or_else(super::invalid)?;
            if target != &original {
                return Err(Error::new(
                    ErrorKind::Configuration,
                    "authentication target changed before commit",
                ));
            }
            *target = updated;
            Ok(())
        },
    )?;
    crate::output::json(
        &mut std::io::stdout().lock(),
        &json!({
            "schema_version": 1, "auth": {"server": args.server, "state": "configured", "remote_validity_checked": false},
            "project_reapproval_required": report.project_reapproval_required,
            "shadowed_by_trusted_project": report.shadowed_by_trusted_project
        }),
    )
}

fn needs_secret(args: &AuthConfigure) -> bool {
    args.source.bearer
        || args.client_secret
        || (args.source.header.is_some() && args.header_env.is_none())
}

pub(super) fn validate_secret(value: String) -> Result<String, Error> {
    if value.is_empty() || value.len() > 16384 || value.chars().any(char::is_control) {
        return Err(Error::new(
            ErrorKind::Authentication,
            "credential must be a nonempty single-line value",
        ));
    }
    Ok(value)
}

async fn save(
    store: &dyn SecureStore,
    secret: &Secret,
    operation: &Operation,
) -> Result<SecretReference, Error> {
    let key: String = crate::config::digest(super::random()?.as_bytes());
    let bytes: Vec<u8> = serde_json::to_vec(secret.expose()).map_err(crate::storage::io_error)?;
    operation.run(store.write(&key, &bytes, true)).await?;
    if operation.run(store.read(&key)).await?.as_deref() != Some(bytes.as_slice()) {
        return Err(super::invalid());
    }
    Ok(SecretReference::Stored(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::support::{Memory, operation};
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn stored_setup_uses_unique_opaque_references_and_verifies_writes() {
        let store: Memory = Memory::default();
        let secret: Secret = Secret::new("private-token".to_owned());
        let first: SecretReference = save(&store, &secret, &operation()).await.unwrap();
        let second: SecretReference = save(&store, &secret, &operation()).await.unwrap();
        assert!(first != second);
        let SecretReference::Stored(key): SecretReference = first else {
            panic!("expected stored reference");
        };
        assert!(crate::config::authentication::stored_key(&key));
        let bytes: Vec<u8> = store.read(&key).await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<String>(&bytes).unwrap(),
            secret.expose()
        );
        let reference: String = serde_json::to_string(&second).unwrap();
        assert!(!reference.contains("private-token"));
        store.fail_write.store(3, Ordering::SeqCst);
        assert!(save(&store, &secret, &operation()).await.is_err());
        store.fail_read.store(true, Ordering::SeqCst);
        assert!(save(&store, &secret, &operation()).await.is_err());
    }
}
