mod effective;
mod mutation;
pub mod schema;
pub(crate) mod unique;

pub use effective::{AuthorizedServer, Effective, Identity, ServerReport, Source};
pub use mutation::{MutationReport, WriteScope};
pub use schema::{Configuration, Definition, Server};

use crate::error::{Error, ErrorKind};
use crate::storage::{Directory, Location, Paths, Store};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct Project {
    root: PathBuf,
    bytes: Vec<u8>,
    configuration: Configuration,
    digest: String,
}

impl Project {
    pub fn discover(start: &Path) -> Result<Option<Self>, Error> {
        let start: PathBuf = start.canonicalize().map_err(crate::storage::io_error)?;
        if !start.is_dir() {
            return Err(Error::new(
                ErrorKind::Configuration,
                "project discovery requires a directory",
            ));
        }
        for root in start.ancestors() {
            let Some(directory): Option<Directory> =
                Directory::open(&root.join(".ripmcp"), false, false)?
            else {
                continue;
            };
            if let Some(bytes) = directory.read("config.json", false)? {
                return Self::from_bytes(root.to_path_buf(), bytes).map(Some);
            }
        }
        Ok(None)
    }

    fn from_bytes(root: PathBuf, bytes: Vec<u8>) -> Result<Self, Error> {
        let configuration: Configuration = Configuration::parse(&bytes)?;
        let digest: String = digest(&bytes);
        Ok(Self {
            root,
            bytes,
            configuration,
            digest,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn user_store(paths: &Paths) -> Result<Store, Error> {
    Store::new(paths.directory(Location::Config)?, "config.json", false)?.recording(paths)
}
