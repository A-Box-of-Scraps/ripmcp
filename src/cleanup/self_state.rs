use crate::{
    error::Error,
    mcp::Operation,
    ownership::{
        Filesystem, Journal, OwnershipStore,
        artifacts::{self, Artifacts},
    },
    storage::{Location, Paths, Store},
};
use serde_json::{Value, json};

pub(crate) fn ownership(paths: &Paths) -> Result<Journal, Error> {
    // The marker remains the recovery source after journals are removed, until binary removal succeeds.
    if Store::new(paths.directory(Location::State)?, "ownership.json", true)?
        .read()?
        .is_some()
    {
        return OwnershipStore::new(paths)?.read();
    }
    let journal: Journal = backup(paths, "ownership_backup")?
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| super::changed())?
        .unwrap_or_default();
    journal.validate()?;
    Ok(journal)
}

pub(crate) fn artifacts(paths: &Paths) -> Result<Artifacts, Error> {
    if artifacts::store(paths)?.read()?.is_some() {
        return artifacts::read(paths);
    }
    let artifacts: Artifacts = backup(paths, "artifacts_backup")?
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| super::changed())?
        .unwrap_or_default();
    artifacts.validate()?;
    Ok(artifacts)
}

fn backup(paths: &Paths, key: &str) -> Result<Option<Value>, Error> {
    let bytes: Option<Vec<u8>> =
        Store::new(paths.directory(Location::State)?, "self-removal.json", true)?.read()?;
    let value: Option<Value> = bytes
        .as_deref()
        .map(crate::json::parse)
        .transpose()
        .map_err(|_| super::changed())?;
    Ok(value.and_then(|mut value| value.as_object_mut()?.remove(key)))
}

pub(crate) async fn save(
    paths: &Paths,
    store: &Store,
    report: &Value,
    operation: &Operation,
) -> Result<(), Error> {
    let artifacts: Artifacts = artifacts(paths)?;
    let ledger_filesystem: Option<Filesystem> =
        refresh_ledger(paths, &artifacts, operation).await?;
    let value: Value = json!({"schema_version": 1, "report": report, "ownership_backup": ownership(paths)?, "artifacts_backup": artifacts, "ledger_filesystem": ledger_filesystem});
    super::self_remove::save(store, &value, operation).await
}

async fn refresh_ledger(
    paths: &Paths,
    artifacts: &Artifacts,
    operation: &Operation,
) -> Result<Option<Filesystem>, Error> {
    if artifacts::store(paths)?.read()?.is_none() {
        return ledger_filesystem(paths);
    }
    let locked: crate::storage::LockedStore = artifacts::store(paths)?.lock(operation).await?;
    let bytes: &[u8] = locked.previous.as_deref().ok_or_else(super::changed)?;
    if crate::json::parse(bytes).map_err(|_| super::changed())?
        != serde_json::to_value(artifacts).map_err(crate::storage::io_error)?
    {
        return Err(super::changed());
    }
    locked.commit_created(bytes, operation).map(Some)
}

pub(crate) fn ledger_filesystem(paths: &Paths) -> Result<Option<Filesystem>, Error> {
    backup(paths, "ledger_filesystem")?
        .filter(|value| !value.is_null())
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| super::changed())
}

pub(crate) async fn restore(paths: &Paths, operation: &Operation) -> Result<(), Error> {
    if artifacts::store(paths)?.read()?.is_none() {
        let value: Value =
            serde_json::to_value(artifacts(paths)?).map_err(crate::storage::io_error)?;
        super::self_remove::save(&artifacts::store(paths)?, &value, operation).await?;
    }
    if Store::new(paths.directory(Location::State)?, "ownership.json", true)?
        .read()?
        .is_none()
    {
        let journal: Journal = ownership(paths)?;
        OwnershipStore::new(paths)?
            .update_async(operation, |current| {
                *current = journal;
                Ok(())
            })
            .await?;
    }
    Ok(())
}
