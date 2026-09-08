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
}

impl Store {
    pub fn new(directory: PathBuf, name: &str, private: bool) -> Result<Self, Error> {
        if name.is_empty() || name.contains('/') || name.starts_with('.') {
            return Err(invalid());
        }
        Ok(Self {
            directory,
            name: name.to_owned(),
            private,
        })
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
        let lock_name: String = format!(".{}.lock", self.name);
        let lock: File = directory
            .file(&lock_name, OFlags::RDWR | OFlags::CREATE, true)?
            .ok_or_else(invalid)?;
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
        directory.replace(&temporary, &self.name)?;
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
