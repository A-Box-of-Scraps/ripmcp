use crate::{
    error::{Error, ErrorKind},
    ownership::{Filesystem, Resource, ResourceIdentity},
    storage::Directory,
};
use std::{
    fs::Metadata,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(crate) fn remove(resource: &Resource) -> Result<(), Error> {
    let ResourceIdentity::Path {
        canonical_path: path,
    }: &ResourceIdentity = &resource.identity
    else {
        return Err(super::changed());
    };
    let proof: &Filesystem = resource.filesystem.as_ref().ok_or_else(super::changed)?;
    let Some(parent): Option<Directory> =
        Directory::open(path.parent().ok_or_else(super::changed)?, false, false)?
    else {
        return Ok(());
    };
    let metadata: Metadata = std::fs::metadata(parent.descriptor_path()).map_err(cause)?;
    if (metadata.dev(), metadata.ino()) != (proof.parent_device, proof.parent_inode) {
        return Err(super::changed());
    }
    let target: PathBuf = parent
        .descriptor_path()
        .join(path.file_name().ok_or_else(super::changed)?);
    let quarantine: PathBuf = parent
        .descriptor_path()
        .join(format!(".ripmcp-remove-{}-{}", proof.device, proof.inode));
    if metadata_at(&target)?.is_none() && metadata_at(&quarantine)?.is_none() {
        return Ok(());
    }
    stage(&target, &quarantine, proof)?;
    if proof.directory {
        std::fs::remove_dir(&quarantine).map_err(cause)?;
    } else {
        std::fs::remove_file(&quarantine).map_err(cause)?;
    }
    parent.sync()
}

fn stage(target: &Path, quarantine: &Path, proof: &Filesystem) -> Result<(), Error> {
    if let Some(metadata) = metadata_at(quarantine)? {
        if proof.matches(&metadata) && metadata_at(target)?.is_none() {
            return Ok(());
        }
        return Err(super::changed());
    }
    let metadata: Metadata = metadata_at(target)?.ok_or_else(super::changed)?;
    if !proof.matches(&metadata) {
        return Err(super::changed());
    }
    // Rename without replacement, then validate the moved object before unlinking it.
    rename(target, quarantine)?;
    let moved: Metadata = metadata_at(quarantine)?.ok_or_else(super::changed)?;
    if !proof.matches(&moved) {
        rename(quarantine, target)?;
        return Err(super::changed());
    }
    if proof.directory {
        let empty = std::fs::read_dir(quarantine)
            .map_err(cause)?
            .next()
            .is_none();
        if !empty {
            rename(quarantine, target)?;
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "directory contains untracked or preserved entries; no recursive deletion performed",
            ));
        }
    }
    Ok(())
}

fn rename(source: &Path, target: &Path) -> Result<(), Error> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        target,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(cause)
}

fn metadata_at(path: &Path) -> Result<Option<Metadata>, Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(cause(error)),
    }
}

fn cause(error: impl std::fmt::Display) -> Error {
    Error {
        kind: ErrorKind::PartialFailure,
        message: format!("filesystem cleanup failed: {error}").into(),
    }
}
