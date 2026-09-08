use super::{invalid, wire};
use crate::error::Error;
use crate::ownership::InstallationId;
use crate::storage::{Directory, Location, Paths};
use rustix::fs::OFlags;
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata};
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};

const SOCKET: &str = "supervisor.sock";
const RECORD: &str = "supervisor.json";
const PENDING_SOCKET: &str = ".supervisor.socket.pending";

pub(super) struct Endpoint {
    directory: Directory,
    pub context: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub protocol: u32,
    pub build: String,
    pub nonce: InstallationId,
    pub context: String,
    pub pid: u32,
    device: u64,
    inode: u64,
}

impl Endpoint {
    pub fn open(paths: &Paths, create: bool) -> Result<Option<Self>, Error> {
        let Some(directory): Option<Directory> = paths.runtime(create)? else {
            return Ok(None);
        };
        let locations: [PathBuf; 3] = [
            paths.directory(Location::Config)?,
            paths.directory(Location::Data)?,
            paths.directory(Location::State)?,
        ];
        let bytes: Vec<u8> = serde_json::to_vec(&locations).map_err(|_| invalid())?;
        Ok(Some(Self {
            directory,
            context: crate::config::digest(&bytes),
        }))
    }

    pub fn path(&self) -> PathBuf {
        self.directory.descriptor_path().join(SOCKET)
    }

    pub fn lock(&self, name: &str) -> Result<File, Error> {
        self.directory
            .file(name, OFlags::RDWR | OFlags::CREATE, true)?
            .ok_or_else(invalid)
    }

    pub fn record(&self) -> Result<Option<Record>, Error> {
        self.directory
            .read(RECORD, true)?
            .map(|bytes| {
                let record: Record = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                if record.protocol != wire::VERSION || record.build != env!("CARGO_PKG_VERSION") {
                    return Err(super::incompatible());
                }
                if record.context != self.context || record.pid == 0 {
                    return Err(invalid());
                }
                Ok(record)
            })
            .transpose()
    }

    pub fn metadata(&self) -> Result<Option<Metadata>, Error> {
        self.metadata_at(SOCKET)
    }

    fn metadata_at(&self, name: &str) -> Result<Option<Metadata>, Error> {
        let metadata: Metadata =
            match std::fs::symlink_metadata(self.directory.descriptor_path().join(name)) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(invalid()),
            };
        if !metadata.file_type().is_socket()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o600
            || metadata.nlink() != 1
        {
            return Err(invalid());
        }
        Ok(Some(metadata))
    }

    pub fn verify(&self, record: &Record) -> Result<(), Error> {
        self.verify_at(record, SOCKET)
    }

    fn verify_at(&self, record: &Record, name: &str) -> Result<(), Error> {
        let metadata: Metadata = self.metadata_at(name)?.ok_or_else(invalid)?;
        if metadata.dev() != record.device || metadata.ino() != record.inode {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn authenticate(stream: &UnixStream, pid: Option<u32>) -> Result<(), Error> {
        let credentials: tokio::net::unix::UCred = stream.peer_cred().map_err(|_| invalid())?;
        validate_peer(credentials.uid(), credentials.pid(), pid)
    }

    pub fn bind(&self) -> Result<(UnixListener, Record), Error> {
        let pending: PathBuf = self.directory.descriptor_path().join(PENDING_SOCKET);
        let listener: UnixListener = UnixListener::bind(&pending).map_err(|_| invalid())?;
        std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| invalid())?;
        let metadata: Metadata = std::fs::symlink_metadata(&pending).map_err(|_| invalid())?;
        let record: Record = Record {
            protocol: wire::VERSION,
            build: env!("CARGO_PKG_VERSION").to_owned(),
            nonce: InstallationId::new()?,
            context: self.context.clone(),
            pid: std::process::id(),
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        self.save(&record)?;
        self.directory.replace(PENDING_SOCKET, SOCKET)?;
        Ok((listener, record))
    }

    fn save(&self, record: &Record) -> Result<(), Error> {
        self.directory.remove(".supervisor.pending")?;
        let mut file: File = self
            .directory
            .file(
                ".supervisor.pending",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
                true,
            )?
            .ok_or_else(invalid)?;
        let bytes: Vec<u8> = serde_json::to_vec(record).map_err(|_| invalid())?;
        file.write_all(&bytes).map_err(crate::storage::io_error)?;
        file.sync_all().map_err(crate::storage::io_error)?;
        self.directory.replace(".supervisor.pending", RECORD)
    }

    pub async fn reconcile(&self) -> Result<(), Error> {
        let record: Option<Record> = self.record()?;
        let name: &str = if self.metadata()?.is_some() {
            SOCKET
        } else {
            PENDING_SOCKET
        };
        if self.metadata_at(name)?.is_none() {
            return Ok(());
        }
        let record: Record = record.ok_or_else(invalid)?;
        self.verify_at(&record, name)?;
        match UnixStream::connect(self.directory.descriptor_path().join(name)).await {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                self.verify_at(&record, name)?;
                self.directory.remove(name)?;
                self.directory.remove(RECORD)
            }
            _ => Err(invalid()),
        }
    }

    pub fn remove(&self, record: &Record) -> Result<(), Error> {
        self.verify(record)?;
        self.directory.remove(SOCKET)?;
        self.directory.remove(RECORD)
    }
}

fn validate_peer(uid: u32, pid: Option<i32>, expected: Option<u32>) -> Result<(), Error> {
    if uid != rustix::process::geteuid().as_raw()
        || expected
            .is_some_and(|expected| pid.and_then(|pid| u32::try_from(pid).ok()) != Some(expected))
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_peer;

    #[test]
    fn peer_identity_checks_owner_and_recorded_process() {
        let uid = rustix::process::geteuid().as_raw();
        assert!(validate_peer(uid, Some(123), Some(123)).is_ok());
        assert!(validate_peer(uid, Some(123), None).is_ok());
        assert!(validate_peer(uid.wrapping_add(1), Some(123), None).is_err());
        assert!(validate_peer(uid, Some(124), Some(123)).is_err());
        assert!(validate_peer(uid, None, Some(123)).is_err());
        assert!(validate_peer(uid, None, Some(u32::MAX)).is_err());
    }
}
