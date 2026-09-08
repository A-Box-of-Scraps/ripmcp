use crate::call::Input;
use crate::error::{Error, ErrorKind};
use crate::mcp::Operation;
use rustix::fs::{Mode, OFlags};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tokio::io::unix::{AsyncFd, AsyncFdReadyGuard};

const LIMIT: usize = 16 * 1024 * 1024;

pub(crate) async fn arguments(input: Input, operation: &Operation) -> Result<String, Error> {
    operation.remaining()?;
    let value: Value = match input {
        Input::Inline(map) => Value::Object(map),
        input => {
            let path: PathBuf = match input {
                Input::File(path) => path,
                _ => PathBuf::from("/dev/stdin"),
            };
            let bytes: Vec<u8> = read(path, LIMIT, operation).await?;
            crate::json::parse(&bytes).map_err(|_| invalid())?
        }
    };
    if !value.is_object() {
        return Err(invalid());
    }
    let encoded: String = serde_json::to_string(&value).map_err(crate::storage::io_error)?;
    operation.remaining()?;
    Ok(encoded)
}

pub(crate) async fn read(
    path: PathBuf,
    limit: usize,
    operation: &Operation,
) -> Result<Vec<u8>, Error> {
    let path: PathBuf = if path.as_os_str() == "-" {
        PathBuf::from("/dev/stdin")
    } else {
        path
    };
    let file: File = rustix::fs::open(
        &path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|_| Error::new(ErrorKind::Io, "cannot open JSON input"))?;
    if file.metadata().map_err(crate::storage::io_error)?.is_file() {
        synchronous(file, limit)
    } else {
        operation.run(stream(file, limit)).await
    }
}

fn synchronous(file: File, limit: usize) -> Result<Vec<u8>, Error> {
    let mut bytes: Vec<u8> = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::storage::io_error)?;
    if bytes.len() > limit {
        return Err(Error::new(
            ErrorKind::Usage,
            "JSON input exceeds size limit",
        ));
    }
    Ok(bytes)
}

async fn stream(file: File, limit: usize) -> Result<Vec<u8>, Error> {
    let fd: AsyncFd<File> = match AsyncFd::new(file.try_clone().map_err(crate::storage::io_error)?)
    {
        Ok(fd) => fd,
        Err(error) if error.raw_os_error() == Some(rustix::io::Errno::PERM.raw_os_error()) => {
            return synchronous(file, limit);
        }
        Err(error) => return Err(crate::storage::io_error(error)),
    };
    let mut bytes: Vec<u8> = Vec::new();
    let mut buffer: [u8; 8192] = [0; 8192];
    loop {
        let mut ready: AsyncFdReadyGuard<'_, File> =
            fd.readable().await.map_err(crate::storage::io_error)?;
        let count = match ready
            .try_io(|fd| rustix::io::read(fd.get_ref(), &mut buffer).map_err(Into::into))
        {
            Ok(result) => result.map_err(crate::storage::io_error)?,
            Err(_) => continue,
        };
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > limit {
            return Err(Error::new(
                ErrorKind::Usage,
                "JSON input exceeds size limit",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn invalid() -> Error {
    Error::new(
        ErrorKind::Usage,
        "expected a bounded JSON object for tool arguments",
    )
}
