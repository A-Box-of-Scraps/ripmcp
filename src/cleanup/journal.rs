use super::{
    Report,
    plan::{Plan, preservation},
};
use crate::{
    error::Error,
    mcp::Operation,
    ownership::{
        CleanupState, InstallationId, Journal, OperationKind, OperationState, OwnershipStore,
        Registration, ResourceIdentity,
    },
    storage::Paths,
};

pub(crate) async fn begin(
    paths: &Paths,
    plan: &Plan,
    clean: bool,
    operation: &Operation,
) -> Result<(), Error> {
    let Some(id): &Option<String> = &plan.installation else {
        return Ok(());
    };
    OwnershipStore::new(paths)?
        .update_async(operation, |journal| {
            if crate::config::digest(
                &serde_json::to_vec(&journal).map_err(crate::storage::io_error)?,
            ) != plan.ownership_digest
            {
                return Err(super::changed());
            }
            let installation_id: InstallationId = journal
                .installations
                .get(id)
                .ok_or_else(super::changed)?
                .id()
                .clone();
            journal.operations.insert(
                id.clone(),
                crate::ownership::Operation {
                    id: installation_id.clone(),
                    installation_id,
                    kind: if clean {
                        OperationKind::Clean
                    } else {
                        OperationKind::Unregister
                    },
                    state: OperationState::Applying,
                    resources: plan.deletable.clone(),
                },
            );
            Ok(())
        })
        .await
}

pub(crate) async fn finish(
    paths: &Paths,
    plan: &Plan,
    operation: &Operation,
    report: &Report,
) -> Result<(), Error> {
    let Some(id): &Option<String> = &plan.installation else {
        return Ok(());
    };
    OwnershipStore::new(paths)?
        .update_async(operation, |journal| {
            let entry: &mut crate::ownership::Operation =
                journal.operations.get_mut(id).ok_or_else(super::changed)?;
            entry.state = if report.failures.is_empty() {
                OperationState::Committed
            } else {
                OperationState::RetryRequired
            };
            Ok(())
        })
        .await
}

pub(crate) async fn unregistered(
    paths: &Paths,
    plan: &Plan,
    operation: &Operation,
) -> Result<(), Error> {
    let Some(id): &Option<String> = &plan.installation else {
        return Ok(());
    };
    OwnershipStore::new(paths)?
        .update_async(operation, |journal| {
            journal
                .installations
                .get_mut(id)
                .ok_or_else(super::changed)?
                .registration = Registration::Unregistered;
            Ok(())
        })
        .await
}

pub(crate) async fn delete(
    paths: &Paths,
    plan: &Plan,
    operation: &Operation,
    report: &mut Report,
) -> Result<(), Error> {
    for identity in &plan.deletable {
        let result: Result<(), Error> = OwnershipStore::new(paths)?
            .update_async(operation, |journal| {
                apply(
                    journal,
                    plan,
                    identity,
                    &crate::ownership::artifacts::read(paths)?,
                )
            })
            .await?;
        match result {
            Ok(()) => report
                .completed
                .push(serde_json::json!({"action": "delete", "resource": identity})),
            Err(error) => report.failure("delete", Some(identity.clone()), &error),
        }
    }
    Ok(())
}

fn apply(
    journal: &mut Journal,
    plan: &Plan,
    identity: &ResourceIdentity,
    artifacts: &crate::ownership::artifacts::Artifacts,
) -> Result<Result<(), Error>, Error> {
    let id: &String = plan.installation.as_ref().ok_or_else(super::changed)?;
    let index = journal
        .installations
        .get(id)
        .ok_or_else(super::changed)?
        .resources
        .iter()
        .position(|resource| &resource.identity == identity)
        .ok_or_else(super::changed)?;
    let resource: &crate::ownership::Resource = &journal.installations[id].resources[index];
    if preservation(journal, resource, artifacts).is_some() {
        return Ok(Err(super::changed()));
    }
    let result: Result<(), Error> = super::remove::remove(resource);
    journal
        .installations
        .get_mut(id)
        .ok_or_else(super::changed)?
        .resources[index]
        .cleanup = if result.is_ok() {
        CleanupState::Removed
    } else {
        CleanupState::RetryRequired
    };
    Ok(result)
}
