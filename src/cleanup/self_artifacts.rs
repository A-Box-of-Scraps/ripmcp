use super::self_plan::SelfPlan;
use crate::{
    error::Error,
    mcp::Operation,
    ownership::{
        CleanupState, Filesystem, Origin, Ownership, Resource, ResourceIdentity, ResourceKind,
        artifacts::{self, Artifacts},
    },
    storage::{Location, Paths},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(crate) async fn clean(
    paths: &Paths,
    plan: &SelfPlan,
    report: &mut Value,
    operation: &Operation,
) -> Result<(), Error> {
    for (path, resource) in &plan.artifacts.files {
        if matches!(
            resource.cleanup,
            CleanupState::Removed | CleanupState::Preserved
        ) {
            continue;
        }
        let current: Artifacts = artifacts::read(paths)?;
        let target: &Resource = if path == &plan.user_config {
            if let Some(bytes) = crate::config::user_store(paths)?.read()?
                && Some(crate::config::digest(&bytes).as_str())
                    != report["configuration_after"].as_str()
            {
                return Err(super::changed());
            }
            current.files.get(path).ok_or_else(super::changed)?
        } else {
            resource
        };
        let result: Result<(), Error> = super::remove::remove(target);
        match result {
            Ok(()) => {
                let mut current: Artifacts = current;
                current
                    .files
                    .get_mut(path)
                    .ok_or_else(super::changed)?
                    .cleanup = CleanupState::Removed;
                super::self_remove::save(
                    &artifacts::store(paths)?,
                    &serde_json::to_value(current).map_err(crate::storage::io_error)?,
                    operation,
                )
                .await?;
                if let Some(completed) = report["completed"].as_array_mut() {
                    completed.push(json!({"action": "delete_application_artifact", "path": path}));
                }
            }
            Err(error) => super::self_remove::append_failure(
                report,
                "delete_application_artifact",
                &Error {
                    kind: error.kind,
                    message: format!("{}: {}", path.display(), error.message).into(),
                },
            ),
        }
    }
    Ok(())
}

pub(crate) fn metadata(paths: &Paths) -> Result<(), Error> {
    let state: PathBuf = paths.directory(Location::State)?;
    let artifacts: Artifacts = artifacts::read(paths)?;
    let ownership: PathBuf = state.join("ownership.json");
    let resource: &Resource = artifacts.files.get(&ownership).ok_or_else(super::changed)?;
    super::remove::remove(resource)?;
    for lock in [
        state.join(".ownership.json.lock"),
        paths.directory(Location::Config)?.join(".config.json.lock"),
    ] {
        if let Some(resource) = artifacts.files.get(&lock) {
            super::remove::remove(resource)?;
        }
    }
    let path: PathBuf = state.join("artifacts.json");
    let filesystem: Filesystem =
        super::self_state::ledger_filesystem(paths)?.ok_or_else(super::changed)?;
    if filesystem.root != state || filesystem.directory {
        return Err(super::changed());
    }
    remove_created(&path, filesystem)
}

fn remove_created(path: &Path, filesystem: Filesystem) -> Result<(), Error> {
    let resource: Resource = Resource {
        kind: ResourceKind::Data,
        identity: ResourceIdentity::Path {
            canonical_path: path.to_path_buf(),
        },
        origin: Origin::Standalone,
        ownership: Ownership::Exclusive,
        cleanup: CleanupState::Pending,
        filesystem: Some(filesystem),
    };
    super::remove::remove(&resource)
}
