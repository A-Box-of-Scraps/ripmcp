use super::{Directory, invalid, io_error};
use crate::deadline::Deadline;
use crate::error::Error;
use rustix::fs::OFlags;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub struct Store {
    directory: PathBuf,
    name: String,
    private: bool,
    ledger: Option<super::Paths>,
}

pub(crate) struct LockedStore {
    directory: Directory,
    name: String,
    private: bool,
    _lock: File,
    path: PathBuf,
    pub previous: Option<Vec<u8>>,
    ledger: Option<super::Paths>,
}

impl LockedStore {
    pub fn unchanged(&self) -> Result<(), Error> {
        use std::os::unix::fs::MetadataExt;
        let current: Directory =
            Directory::open(&self.path, false, self.private)?.ok_or_else(invalid)?;
        let expected: std::fs::Metadata =
            std::fs::metadata(self.directory.descriptor_path()).map_err(io_error)?;
        let actual: std::fs::Metadata =
            std::fs::metadata(current.descriptor_path()).map_err(io_error)?;
        if (actual.dev(), actual.ino()) != (expected.dev(), expected.ino()) {
            return Err(invalid());
        }
        if self.directory.read(&self.name, self.private)? != self.previous {
            return Err(crate::error::Error::new(
                crate::error::ErrorKind::Configuration,
                "configuration changed during installation; retry without overwriting it",
            ));
        }
        Ok(())
    }

    pub fn commit(&self, bytes: &[u8], operation: &crate::mcp::Operation) -> Result<(), Error> {
        self.commit_created(bytes, operation).map(|_| ())
    }

    pub(crate) fn commit_created(
        &self,
        bytes: &[u8],
        operation: &crate::mcp::Operation,
    ) -> Result<crate::ownership::Filesystem, Error> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid());
        }
        self.unchanged()?;
        let temporary: String = format!(".{}.pending", self.name);
        self.directory.remove(&temporary)?;
        let mut file: File = self
            .directory
            .file(
                &temporary,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
                true,
            )?
            .ok_or_else(invalid)?;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        operation.remaining()?;
        self.unchanged()?;
        let filesystem: crate::ownership::Filesystem = crate::ownership::Filesystem::from_created(
            &self.path.join(&self.name),
            &self.path,
            &file.metadata().map_err(io_error)?,
            &self.directory.metadata()?,
        )?;
        self.directory.replace(&temporary, &self.name)?;
        record(
            &self.ledger,
            &self.path.join(&self.name),
            filesystem.clone(),
            &Deadline::new(operation.remaining()?),
        )?;
        Ok(filesystem)
    }
}

impl Store {
    pub(crate) async fn lock(
        &self,
        operation: &crate::mcp::Operation,
    ) -> Result<LockedStore, Error> {
        operation.remaining()?;
        let directory: Directory =
            Directory::open(&self.directory, true, self.private)?.ok_or_else(invalid)?;
        let lock: File = lock_file(
            &directory,
            &self.name,
            &self.ledger,
            &Deadline::new(operation.remaining()?),
        )?;
        operation.run(acquire_async(&lock)).await?;
        let previous: Option<Vec<u8>> = directory.read(&self.name, self.private)?;
        Ok(LockedStore {
            directory,
            name: self.name.clone(),
            private: self.private,
            _lock: lock,
            path: self.directory.clone(),
            previous,
            ledger: self.ledger.clone(),
        })
    }
    pub fn new(directory: PathBuf, name: &str, private: bool) -> Result<Self, Error> {
        if name.is_empty() || name.contains('/') || name.starts_with('.') {
            return Err(invalid());
        }
        Ok(Self {
            directory,
            name: name.to_owned(),
            private,
            ledger: None,
        })
    }

    pub(crate) fn recording(mut self, paths: &super::Paths) -> Result<Self, Error> {
        self.ledger = Some(paths.clone());
        Ok(self)
    }

    pub fn read(&self) -> Result<Option<Vec<u8>>, Error> {
        let Some(directory): Option<Directory> =
            Directory::open(&self.directory, false, self.private)?
        else {
            return Ok(None);
        };
        directory.read(&self.name, self.private)
    }

    pub fn update<T>(
        &self,
        change: impl FnOnce(Option<&[u8]>) -> Result<(Vec<u8>, T), Error>,
    ) -> Result<T, Error> {
        self.update_with_deadline(&Deadline::new(Duration::from_secs(60)), change)
    }

    pub fn update_with_deadline<T>(
        &self,
        deadline: &Deadline,
        change: impl FnOnce(Option<&[u8]>) -> Result<(Vec<u8>, T), Error>,
    ) -> Result<T, Error> {
        deadline.remaining()?;
        let directory: Directory =
            Directory::open(&self.directory, true, self.private)?.ok_or_else(invalid)?;
        let lock: File = lock_file(&directory, &self.name, &self.ledger, deadline)?;
        acquire(&lock, deadline)?;
        let previous: Option<Vec<u8>> = directory.read(&self.name, self.private)?;
        let (bytes, result): (Vec<u8>, T) = change(previous.as_deref())?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid());
        }
        if previous.as_deref() == Some(bytes.as_slice()) {
            return Ok(result);
        }
        let temporary: String = format!(".{}.pending", self.name);
        // Only the lock holder can discard an interrupted, never-committed write.
        directory.remove(&temporary)?;
        let mut file: File = directory
            .file(
                &temporary,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
                true,
            )?
            .ok_or_else(invalid)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        deadline.remaining()?;
        let filesystem: crate::ownership::Filesystem = crate::ownership::Filesystem::from_created(
            &self.directory.join(&self.name),
            &self.directory,
            &file.metadata().map_err(io_error)?,
            &directory.metadata()?,
        )?;
        directory.replace(&temporary, &self.name)?;
        record(
            &self.ledger,
            &self.directory.join(&self.name),
            filesystem,
            deadline,
        )?;
        Ok(result)
    }

    pub fn update_json<T: DeserializeOwned + Serialize + Default, R>(
        &self,
        change: impl FnOnce(&mut T) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.update_json_with_deadline(&Deadline::new(Duration::from_secs(60)), change)
    }

    pub fn update_json_with_deadline<T: DeserializeOwned + Serialize + Default, R>(
        &self,
        deadline: &Deadline,
        change: impl FnOnce(&mut T) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.update_with_deadline(deadline, |previous| {
            let mut value: T = match previous {
                Some(bytes) => serde_json::from_slice(bytes).map_err(|_| invalid())?,
                None => T::default(),
            };
            let result: R = change(&mut value)?;
            let bytes: Vec<u8> = serde_json::to_vec_pretty(&value).map_err(io_error)?;
            Ok((bytes, result))
        })
    }
}

fn record(
    ledger: &Option<super::Paths>,
    path: &std::path::Path,
    filesystem: crate::ownership::Filesystem,
    deadline: &Deadline,
) -> Result<(), Error> {
    if let Some(ledger) = ledger {
        crate::ownership::artifacts::record(
            &ledger.directory(super::Location::State)?,
            path,
            filesystem,
            deadline,
        )?;
    }
    Ok(())
}

fn lock_file(
    directory: &Directory,
    name: &str,
    ledger: &Option<super::Paths>,
    deadline: &Deadline,
) -> Result<File, Error> {
    let name: String = format!(".{name}.lock");
    let file: Option<File> = match ledger {
        Some(paths) => directory.recorded_file(
            &name,
            OFlags::RDWR | OFlags::CREATE,
            &paths.directory(super::Location::State)?,
            deadline,
        )?,
        None => directory.file(&name, OFlags::RDWR | OFlags::CREATE, true)?,
    };
    file.ok_or_else(invalid)
}

fn acquire(file: &File, deadline: &Deadline) -> Result<(), Error> {
    loop {
        let remaining: Duration = deadline.remaining()?;
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(remaining.min(Duration::from_millis(5)))
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(io_error(error)),
        }
    }
}

async fn acquire_async(lock: &File) -> Result<(), Error> {
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                tokio::time::sleep(Duration::from_millis(5)).await
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(io_error(error)),
        }
    }
}
