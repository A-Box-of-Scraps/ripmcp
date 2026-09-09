use super::{Directory, Location, Paths, invalid};
use crate::{
    error::{Error, ErrorKind},
    mcp::Operation,
};
use rustix::fs::OFlags;
use std::{
    fs::{File, Metadata, TryLockError},
    os::unix::fs::MetadataExt,
    time::Duration,
};

pub(crate) struct Maintenance {
    directory: Directory,
    file: File,
}

impl Maintenance {
    pub async fn acquire(
        paths: &Paths,
        exclusive: bool,
        operation: &Operation,
    ) -> Result<Self, Error> {
        let directory: Directory =
            Directory::open(&paths.directory(Location::State)?, true, true)?.ok_or_else(invalid)?;
        let file: File = directory
            .file(".maintenance.lock", OFlags::RDWR | OFlags::CREATE, true)?
            .ok_or_else(invalid)?;
        operation.run(acquire_file(&file, exclusive)).await?;
        let current: Directory = Directory::open(&paths.directory(Location::State)?, false, true)?
            .ok_or_else(invalid)?;
        let current: File = current
            .file(".maintenance.lock", OFlags::RDONLY, true)?
            .ok_or_else(invalid)?;
        let expected: Metadata = file.metadata().map_err(super::io_error)?;
        let actual: Metadata = current.metadata().map_err(super::io_error)?;
        if (actual.dev(), actual.ino()) != (expected.dev(), expected.ino()) {
            return Err(invalid());
        }
        if !exclusive && directory.read("self-removal.json", true)?.is_some() {
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "self-removal is pending; retry --uninstall-everything -y before further mutations",
            ));
        }
        Ok(Self { directory, file })
    }

    pub fn finish(self) -> Result<(), Error> {
        self.directory.remove("self-removal.json")?;
        self.directory.remove(".self-removal.json.lock")?;
        self.directory.remove(".maintenance.lock")?;
        self.directory.sync()?;
        drop(self.file);
        Ok(())
    }
}

pub(crate) async fn acquire_file(file: &File, exclusive: bool) -> Result<(), Error> {
    loop {
        let result: Result<(), TryLockError> = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        match result {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => tokio::time::sleep(Duration::from_millis(5)).await,
            Err(_) => return Err(invalid()),
        }
    }
}
