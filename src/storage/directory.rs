use super::{invalid, io_error};
use crate::error::Error;
use rustix::fs::{AtFlags, Mode, OFlags, mkdirat, open, openat, renameat, unlinkat};
use rustix::io::Errno;
use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

pub struct Directory(File);

impl Directory {
    pub(crate) fn sync(&self) -> Result<(), Error> {
        self.0.sync_all().map_err(io_error)
    }
    pub fn open(path: &Path, create: bool, private: bool) -> Result<Option<Self>, Error> {
        if !path.is_absolute() {
            return Err(invalid());
        }
        let mut current: File = open("/", directory_flags(), Mode::empty())
            .map(File::from)
            .map_err(io_error)?;
        for part in path.components() {
            let name: &OsStr = match part {
                Component::RootDir => continue,
                Component::Normal(name) => name,
                _ => return Err(invalid()),
            };
            let Some(next): Option<File> = descend(&current, name, create)? else {
                return Ok(None);
            };
            current = next;
        }
        let metadata: Metadata = current.metadata().map_err(io_error)?;
        if private {
            private_mode(metadata.uid(), metadata.mode(), 0o700)?;
        }
        Ok(Some(Self(current)))
    }

    pub fn read(&self, name: &str, private: bool) -> Result<Option<Vec<u8>>, Error> {
        let Some(file): Option<File> = self.file(name, OFlags::RDONLY, private)? else {
            return Ok(None);
        };
        let mut bytes: Vec<u8> = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid());
        }
        Ok(Some(bytes))
    }

    pub(crate) fn file(
        &self,
        name: &str,
        flags: OFlags,
        private: bool,
    ) -> Result<Option<File>, Error> {
        if name.is_empty() || name.contains('/') || matches!(name, "." | "..") {
            return Err(invalid());
        }
        let file: File = match openat(
            &self.0,
            name,
            flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => File::from(fd),
            Err(Errno::NOENT) => return Ok(None),
            Err(_) => return Err(invalid()),
        };
        let metadata: Metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err(invalid());
        }
        if private {
            private_mode(metadata.uid(), metadata.mode(), 0o600)?;
        }
        Ok(Some(file))
    }

    pub(crate) fn replace(&self, source: &str, target: &str) -> Result<(), Error> {
        renameat(&self.0, source, &self.0, target).map_err(io_error)?;
        self.0.sync_all().map_err(io_error)
    }

    pub(crate) fn remove(&self, name: &str) -> Result<(), Error> {
        match unlinkat(&self.0, name, AtFlags::empty()) {
            Ok(()) | Err(Errno::NOENT) => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    pub(crate) fn descriptor_path(&self) -> std::path::PathBuf {
        use std::os::fd::AsRawFd;
        std::path::PathBuf::from(format!("/proc/self/fd/{}", self.0.as_raw_fd()))
    }
}

fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn descend(parent: &File, name: &OsStr, create: bool) -> Result<Option<File>, Error> {
    let file: File = match openat(parent, name, directory_flags(), Mode::empty()) {
        Ok(fd) => File::from(fd),
        Err(Errno::NOENT) if create => {
            match mkdirat(parent, name, Mode::RWXU) {
                Ok(()) => parent.sync_all().map_err(io_error)?,
                Err(Errno::EXIST) => (),
                Err(error) => return Err(io_error(error)),
            }
            openat(parent, name, directory_flags(), Mode::empty())
                .map(File::from)
                .map_err(|_| invalid())?
        }
        Err(Errno::NOENT) => return Ok(None),
        Err(_) => return Err(invalid()),
    };
    let metadata: Metadata = file.metadata().map_err(io_error)?;
    // Root-owned sticky directories permit the validated private /tmp fallback.
    let sticky_root = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
    if metadata.mode() & 0o022 != 0 && !sticky_root {
        return Err(invalid());
    }
    Ok(Some(file))
}

fn private_mode(owner: u32, mode: u32, expected: u32) -> Result<(), Error> {
    if owner != rustix::process::geteuid().as_raw() || mode & 0o777 != expected {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::private_mode;

    #[test]
    fn private_permissions_do_not_make_foreign_ownership_safe() {
        let uid = rustix::process::geteuid().as_raw();
        for mode in [0o600, 0o700] {
            assert!(private_mode(uid, mode, mode).is_ok());
            assert!(private_mode(uid.wrapping_add(1), mode, mode).is_err());
            assert!(private_mode(uid, mode | 0o040, mode).is_err());
            assert!(private_mode(uid, mode | 0o004, mode).is_err());
        }
    }
}
