use crate::{error::Error, storage::Directory};
use serde::{Deserialize, Serialize};
use std::{
    fs::Metadata,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Filesystem {
    pub root: PathBuf,
    pub parent_device: u64,
    pub parent_inode: u64,
    pub device: u64,
    pub inode: u64,
    pub directory: bool,
    #[serde(default)]
    pub symlink: bool,
    #[serde(default)]
    pub created: Option<std::time::SystemTime>,
}

impl Filesystem {
    pub(super) fn validate(&self, identity: &super::ResourceIdentity) -> Result<(), Error> {
        super::ResourceIdentity::Path {
            canonical_path: self.root.clone(),
        }
        .validate()?;
        let super::ResourceIdentity::Path { canonical_path }: &super::ResourceIdentity = identity
        else {
            return Err(super::invalid());
        };
        if canonical_path == &self.root
            || !canonical_path.starts_with(&self.root)
            || self.inode == 0
            || self.parent_inode == 0
        {
            return Err(super::invalid());
        }
        Ok(())
    }
    pub fn created(path: &Path, root: &Path) -> Result<Self, Error> {
        if !path.starts_with(root)
            || path == root
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(super::invalid());
        }
        let parent: Directory =
            Directory::open(path.parent().ok_or_else(super::invalid)?, false, false)?
                .ok_or_else(super::invalid)?;
        let metadata: Metadata = std::fs::symlink_metadata(
            parent
                .descriptor_path()
                .join(path.file_name().ok_or_else(super::invalid)?),
        )
        .map_err(crate::storage::io_error)?;
        let parent_metadata: Metadata = parent.metadata()?;
        Self::from_created(path, root, &metadata, &parent_metadata)
    }

    pub(crate) fn from_created(
        path: &Path,
        root: &Path,
        metadata: &Metadata,
        parent_metadata: &Metadata,
    ) -> Result<Self, Error> {
        if (!metadata.is_file() && !metadata.is_dir() && !metadata.is_symlink())
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || (!metadata.is_symlink() && metadata.mode() & 0o022 != 0)
            || (!metadata.is_dir() && metadata.nlink() != 1)
        {
            return Err(super::invalid());
        }
        let result: Self = Self {
            root: root.to_path_buf(),
            parent_device: parent_metadata.dev(),
            parent_inode: parent_metadata.ino(),
            device: metadata.dev(),
            inode: metadata.ino(),
            directory: metadata.is_dir(),
            symlink: metadata.is_symlink(),
            created: metadata.created().ok(),
        };
        result.validate(&super::ResourceIdentity::Path {
            canonical_path: path.to_path_buf(),
        })?;
        Ok(result)
    }

    pub(crate) fn matches(&self, metadata: &Metadata) -> bool {
        metadata.dev() == self.device
            && metadata.ino() == self.inode
            && metadata.is_dir() == self.directory
            && metadata.is_symlink() == self.symlink
            && (metadata.is_dir()
                || ((metadata.is_file() || metadata.is_symlink()) && metadata.nlink() == 1))
            && metadata.uid() == rustix::process::geteuid().as_raw()
            && (metadata.is_symlink() || metadata.mode() & 0o022 == 0)
            && self
                .created
                .is_some_and(|created| metadata.created().ok() == Some(created))
    }
}
