use super::{
    CleanupState, Filesystem, Origin, Ownership, Resource, ResourceIdentity, ResourceKind,
};
use crate::{error::Error, storage::Directory};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

#[derive(Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Executable {
    Standalone {
        resource: Box<Resource>,
        sha256: String,
        #[serde(default)]
        setup: Vec<Resource>,
    },
    PackageManager {
        manager: String,
    },
    #[default]
    Unknown,
}

impl Executable {
    pub fn installed_copy(path: &Path, installation_root: &Path) -> Result<Self, Error> {
        let filesystem: Filesystem = Filesystem::created(path, installation_root)?;
        if filesystem.directory || filesystem.symlink {
            return Err(super::invalid());
        }
        Ok(Self::Standalone {
            resource: Box::new(Resource {
                kind: ResourceKind::Executable,
                identity: ResourceIdentity::Path {
                    canonical_path: path.to_path_buf(),
                },
                origin: Origin::Standalone,
                ownership: Ownership::Exclusive,
                cleanup: CleanupState::Pending,
                filesystem: Some(filesystem),
            }),
            sha256: digest(path)?,
            setup: Vec::new(),
        })
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Standalone {
                resource,
                sha256,
                setup,
            } => {
                resource.identity.validate()?;
                let filesystem: &Filesystem =
                    resource.filesystem.as_ref().ok_or_else(super::invalid)?;
                filesystem.validate(&resource.identity)?;
                if filesystem.directory
                    || filesystem.symlink
                    || resource.ownership != Ownership::Exclusive
                    || !matches!(resource.kind, ResourceKind::Executable)
                    || !matches!(resource.origin, Origin::Standalone)
                    || sha256.len() != 64
                    || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(super::invalid());
                }
                for link in setup {
                    validate_link(link)?;
                }
                Ok(())
            }
            Self::PackageManager { manager } if manager.is_empty() => Err(super::invalid()),
            _ => Ok(()),
        }
    }

    pub fn record_created_link(&mut self, path: &Path, root: &Path) -> Result<(), Error> {
        let Self::Standalone { setup, .. }: &mut Self = self else {
            return Err(super::invalid());
        };
        let resource: Resource = Resource {
            kind: ResourceKind::Symlink,
            identity: ResourceIdentity::Path {
                canonical_path: path.to_path_buf(),
            },
            origin: Origin::Standalone,
            ownership: Ownership::Exclusive,
            cleanup: CleanupState::Pending,
            filesystem: Some(Filesystem::created(path, root)?),
        };
        validate_link(&resource)?;
        setup.push(resource);
        Ok(())
    }
}

fn validate_link(resource: &Resource) -> Result<(), Error> {
    resource.identity.validate()?;
    let filesystem: &Filesystem = resource.filesystem.as_ref().ok_or_else(super::invalid)?;
    filesystem.validate(&resource.identity)?;
    if !filesystem.symlink
        || filesystem.directory
        || resource.ownership != Ownership::Exclusive
        || !matches!(resource.kind, ResourceKind::Symlink)
        || !matches!(resource.origin, Origin::Standalone)
    {
        return Err(super::invalid());
    }
    Ok(())
}

pub(crate) fn digest(path: &Path) -> Result<String, Error> {
    let parent: Directory =
        Directory::open(path.parent().ok_or_else(super::invalid)?, false, false)?
            .ok_or_else(super::invalid)?;
    let name: &str = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(super::invalid)?;
    let mut file: File = parent
        .file(name, rustix::fs::OFlags::RDONLY, false)?
        .ok_or_else(super::invalid)?;
    let mut hash: Sha256 = Sha256::new();
    let mut buffer: [u8; 65536] = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(crate::storage::io_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
