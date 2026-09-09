use super::{
    CleanupState, Filesystem, Origin, Ownership, Resource, ResourceIdentity, ResourceKind,
};
use crate::{
    deadline::Deadline,
    error::Error,
    storage::{Location, Paths, Store},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifacts {
    pub schema_version: crate::config::schema::Version,
    #[serde(deserialize_with = "crate::config::unique::map")]
    pub files: BTreeMap<PathBuf, Resource>,
}

impl Artifacts {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        for (path, resource) in &self.files {
            if resource.ownership != Ownership::Exclusive
                || !matches!(resource.kind, ResourceKind::Data)
                || !matches!(resource.origin, Origin::Standalone)
                || path.components().any(|part| part.as_os_str() == ".ripmcp")
            {
                return Err(super::invalid());
            }
            if resource.identity
                != (ResourceIdentity::Path {
                    canonical_path: path.clone(),
                })
            {
                return Err(super::invalid());
            }
            resource.identity.validate()?;
            resource
                .filesystem
                .as_ref()
                .ok_or_else(super::invalid)?
                .validate(&resource.identity)?;
        }
        Ok(())
    }
}

pub(crate) fn store(paths: &Paths) -> Result<Store, Error> {
    Store::new(paths.directory(Location::State)?, "artifacts.json", true)
}

pub(crate) fn read(paths: &Paths) -> Result<Artifacts, Error> {
    read_store(&store(paths)?)
}

fn read_store(store: &Store) -> Result<Artifacts, Error> {
    let artifacts: Artifacts = store
        .read()?
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| super::invalid())?
        .unwrap_or_default();
    artifacts.validate()?;
    Ok(artifacts)
}

pub(crate) fn record(
    ledger: &Path,
    path: &Path,
    filesystem: Filesystem,
    deadline: &Deadline,
) -> Result<(), Error> {
    if path.components().any(|part| part.as_os_str() == ".ripmcp") {
        return Ok(());
    }
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
    let store: Store = Store::new(ledger.to_path_buf(), "artifacts.json", true)?;
    store.update_json_with_deadline::<Artifacts, _>(deadline, |artifacts| {
        artifacts.validate()?;
        artifacts.files.insert(path.to_path_buf(), resource);
        artifacts.validate()?;
        Ok(())
    })
}
