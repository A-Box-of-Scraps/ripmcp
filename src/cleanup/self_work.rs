use super::{Report, journal, self_plan::SelfPlan};
use crate::{
    config::{Configuration, Source},
    error::Error,
    mcp::Operation,
    ownership::{OwnershipStore, Registration},
    storage::{LockedStore, Paths},
};
use serde_json::{Value, json};

pub(crate) async fn unregister(
    paths: &Paths,
    plan: &SelfPlan,
    report: &mut Value,
    operation: &Operation,
) -> Result<(), Error> {
    let locked: LockedStore = crate::config::user_store(paths)?.lock(operation).await?;
    if crate::config::digest(locked.previous.as_deref().unwrap_or_default())
        != plan.configuration_digest
    {
        return Err(super::changed());
    }
    let mut configuration: Configuration = locked
        .previous
        .as_deref()
        .map(Configuration::parse)
        .transpose()?
        .unwrap_or_default();
    configuration.servers.clear();
    let bytes: Vec<u8> =
        serde_json::to_vec_pretty(&configuration).map_err(crate::storage::io_error)?;
    if locked.previous.is_some() {
        locked.commit(&bytes, operation)?;
        report["configuration_after"] = json!(crate::config::digest(&bytes));
    }
    OwnershipStore::new(paths)?
        .update_async(operation, |journal| {
            for installation in journal.installations.values_mut().filter(|installation| {
                installation.scope
                    == (Source::User {
                        config: plan.user_config.clone(),
                    })
            }) {
                installation.registration = Registration::Unregistered;
            }
            Ok(())
        })
        .await?;
    if let Some(completed) = report["completed"].as_array_mut() {
        completed.push(json!({"action": "unregister_user_scope"}));
    }
    Ok(())
}

pub(crate) async fn installations(
    paths: &Paths,
    plan: &SelfPlan,
    report: &mut Value,
    operation: &Operation,
) -> Result<(), Error> {
    for plan in &plan.installations {
        let mut entry: Report = Report {
            schema_version: 1,
            completed: Vec::new(),
            failures: Vec::new(),
            plan: plan.clone(),
        };
        if let Err(error) = journal::delete(paths, plan, operation, &mut entry).await {
            entry.failure("cleanup_journal", None, &error);
        }
        if let Err(error) = journal::finish(paths, plan, operation, &entry).await {
            entry.failure("cleanup_journal", None, &error);
        }
        if let Some(failures) = report["failures"].as_array_mut() {
            failures.extend(entry.failures.iter().cloned());
        }
        if let Some(installations) = report["installations"].as_array_mut() {
            installations.push(serde_json::to_value(entry).map_err(crate::storage::io_error)?);
        }
    }
    Ok(())
}

pub(crate) async fn setup(
    paths: &Paths,
    plan: &SelfPlan,
    report: &mut Value,
    operation: &Operation,
) -> Result<(), Error> {
    for link in &plan.setup {
        let result: Result<(), Error> = OwnershipStore::new(paths)?.update_async(operation, |journal| {
            let crate::ownership::Executable::Standalone { setup, .. }: &crate::ownership::Executable = &journal.executable else { return Err(super::changed()); };
            let index = setup.iter().position(|entry| entry.identity == link.identity).ok_or_else(super::changed)?;
            if super::plan::preservation(journal, &setup[index], &crate::ownership::artifacts::read(paths)?).is_some() { return Ok(Err(super::changed())); }
            let result: Result<(), Error> = super::remove::remove(&setup[index]);
            let crate::ownership::Executable::Standalone { setup, .. }: &mut crate::ownership::Executable = &mut journal.executable else { return Err(super::changed()); };
            setup[index].cleanup = if result.is_ok() { crate::ownership::CleanupState::Removed } else { crate::ownership::CleanupState::RetryRequired };
            Ok(result)
        }).await?;
        match result {
            Ok(()) => {
                if let Some(completed) = report["completed"].as_array_mut() {
                    completed.push(
                        json!({"action": "remove_installation_link", "resource": link.identity}),
                    );
                }
            }
            Err(error) => {
                super::self_remove::append_failure(report, "remove_installation_link", &error)
            }
        }
    }
    Ok(())
}
