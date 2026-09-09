pub(crate) mod executable;
mod filesystem;
pub use executable::Executable;
pub(crate) mod artifacts;
mod model;
pub use filesystem::Filesystem;
pub use model::*;

use crate::error::{Error, ErrorKind};
use crate::storage::{Location, Paths, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema_version: crate::config::schema::Version,
    #[serde(deserialize_with = "crate::config::unique::map")]
    pub installations: BTreeMap<String, Installation>,
    #[serde(deserialize_with = "crate::config::unique::map")]
    pub operations: BTreeMap<String, Operation>,
    #[serde(default)]
    pub executable: Executable,
}

pub struct OwnershipStore(Store);
impl OwnershipStore {
    pub fn new(paths: &Paths) -> Result<Self, Error> {
        Ok(Self(
            Store::new(paths.directory(Location::State)?, "ownership.json", true)?
                .recording(paths)?,
        ))
    }
    pub fn read(&self) -> Result<Journal, Error> {
        let journal: Journal = match self.0.read()? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
            None => Journal::default(),
        };
        journal.validate()?;
        Ok(journal)
    }
    pub(crate) async fn update_async<R>(
        &self,
        operation: &crate::mcp::Operation,
        change: impl FnOnce(&mut Journal) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let locked: crate::storage::LockedStore = self.0.lock(operation).await?;
        let mut journal: Journal = locked
            .previous
            .as_deref()
            .map(serde_json::from_slice)
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or_default();
        journal.validate()?;
        let result: R = change(&mut journal)?;
        journal.validate()?;
        let bytes: Vec<u8> =
            serde_json::to_vec_pretty(&journal).map_err(crate::storage::io_error)?;
        locked.commit(&bytes, operation)?;
        Ok(result)
    }

    pub fn update<R>(
        &self,
        change: impl FnOnce(&mut Journal) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.0.update_json::<Journal, _>(|journal| {
            journal.validate()?;
            let result: R = change(journal)?;
            journal.validate()?;
            Ok(result)
        })
    }
}

impl Journal {
    pub(crate) fn resources(&self) -> impl Iterator<Item = &Resource> {
        let setup: &[Resource] = match &self.executable {
            Executable::Standalone { setup, .. } => setup,
            _ => &[],
        };
        self.installations
            .values()
            .flat_map(|entry| &entry.resources)
            .chain(setup)
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        self.executable.validate()?;
        for (key, installation) in &self.installations {
            if key != installation.id().as_str() {
                return Err(invalid());
            }
            installation.validate()?;
        }
        for (key, operation) in &self.operations {
            for resource in &operation.resources {
                resource.validate()?;
            }
            if key != operation.id.as_str()
                || !self
                    .installations
                    .contains_key(operation.installation_id.as_str())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn unregister(&mut self, id: &InstallationId) -> Result<(), Error> {
        let installation: &mut Installation = self
            .installations
            .get_mut(id.as_str())
            .ok_or_else(invalid)?;
        installation.registration = Registration::Unregistered;
        Ok(())
    }
}

fn invalid() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "ownership journal is corrupt or incompatible",
    )
}
