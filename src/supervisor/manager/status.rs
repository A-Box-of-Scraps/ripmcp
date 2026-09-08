use super::{Manager, Slot, State, key};
use crate::config::{Effective, Identity};
use crate::error::Error;
use crate::storage::{Directory, Paths};
use serde_json::{Value, json};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::MutexGuard;

impl Manager {
    pub async fn status(&self, cwd: &Path) -> Result<Value, Error> {
        let effective: Effective = Effective::load(&self.paths, cwd)?;
        let slots: MutexGuard<'_, std::collections::BTreeMap<String, Arc<Slot>>> =
            self.slots.lock().await;
        let mut reports: Vec<Value> = Vec::new();
        for report in effective.reports() {
            let mut value: Value =
                serde_json::to_value(&report).map_err(crate::storage::io_error)?;
            let identity: &Identity = effective.identity(report.name)?;
            let (process, health): (&str, &str) = if effective.is_local(report.name) {
                describe(slots.get(&key(identity)?), &self.paths, identity)?
            } else {
                ("not_managed", "unknown")
            };
            value["process_state"] = json!(process);
            value["health"] = json!(health);
            reports.push(value);
        }
        Ok(json!({"schema_version": 1, "servers": reports}))
    }
}

fn describe(
    slot: Option<&Arc<Slot>>,
    paths: &Paths,
    identity: &Identity,
) -> Result<(&'static str, &'static str), Error> {
    let Some(slot): Option<&Arc<Slot>> = slot else {
        return inactive(paths, identity);
    };
    let state: MutexGuard<'_, State> = match slot.state.try_lock() {
        Ok(state) => state,
        Err(_) => return Ok(("busy", "unknown")),
    };
    if let Some(client) = &state.client {
        if client.is_closed() {
            let inactive: (&str, &str) = inactive(paths, identity)?;
            if inactive.0 == "cleanup_required" {
                return Ok(inactive);
            }
            return Ok((
                "failed",
                if state.configuration_digest == identity.configuration_digest {
                    "failed"
                } else {
                    "stale"
                },
            ));
        }
        let health: &str = if state.configuration_digest != identity.configuration_digest {
            "stale"
        } else if client.has_abandoned() {
            "unknown"
        } else if state.failed {
            "failed"
        } else if state
            .observed
            .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
        {
            "healthy"
        } else {
            "stale"
        };
        return Ok(("running", health));
    }
    let inactive: (&str, &str) = inactive(paths, identity)?;
    if inactive.0 == "cleanup_required" {
        return Ok(inactive);
    }
    if state.failed {
        return Ok(("failed", "failed"));
    }
    Ok(inactive)
}

fn inactive(paths: &Paths, identity: &Identity) -> Result<(&'static str, &'static str), Error> {
    let key: String = key(identity)?;
    if let Some(state) = Directory::open(
        &paths.directory(crate::storage::Location::State)?,
        false,
        true,
    )? && state
        .read(&format!(".instance-{key}.failed"), true)?
        .is_some()
    {
        return Ok(("cleanup_required", "failed"));
    }
    let Some(directory): Option<Directory> = Directory::open(
        &paths.directory(crate::storage::Location::State)?,
        false,
        true,
    )?
    else {
        return Ok(("stopped", "unverified"));
    };
    let Some(lock): Option<File> = directory.file(
        &format!(".instance-{key}.lock"),
        rustix::fs::OFlags::RDWR,
        true,
    )?
    else {
        return Ok(("stopped", "unverified"));
    };
    match lock.try_lock() {
        Ok(()) => Ok(("stopped", "unknown")),
        Err(std::fs::TryLockError::WouldBlock) => Ok(("unknown", "stale")),
        Err(_) => Err(super::super::invalid()),
    }
}

pub(crate) fn offline(paths: &Paths, cwd: &Path) -> Result<Value, Error> {
    let effective: Effective = Effective::load(paths, cwd)?;
    let mut reports: Vec<Value> = Vec::new();
    for report in effective.reports() {
        let mut value: Value = serde_json::to_value(&report).map_err(crate::storage::io_error)?;
        let (process, health): (&str, &str) = if effective.is_local(report.name) {
            inactive(paths, effective.identity(report.name)?)?
        } else {
            ("not_managed", "unknown")
        };
        value["process_state"] = json!(process);
        value["health"] = json!(health);
        reports.push(value);
    }
    Ok(json!({"schema_version": 1, "servers": reports}))
}
