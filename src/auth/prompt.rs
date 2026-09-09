use crate::{
    error::{Error, ErrorKind},
    mcp::Operation,
    trust::secrets::Secret,
};
use rustix::termios::{LocalModes, OptionalActions, Termios};
use std::{fs::File, io::Write};
use tokio::io::unix::{AsyncFd, AsyncFdReadyGuard};

struct Terminal {
    file: File,
    original: Termios,
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _: Result<(), rustix::io::Errno> =
            rustix::termios::tcsetattr(&self.file, OptionalActions::Flush, &self.original);
        let _: std::io::Result<()> = writeln!(&self.file);
    }
}

pub(super) async fn read(operation: &Operation) -> Result<Secret, Error> {
    let file: File = rustix::fs::open(
        "/dev/tty",
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOCTTY,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(|_| unavailable())?;
    read_terminal(file, operation).await
}

async fn read_terminal(file: File, operation: &Operation) -> Result<Secret, Error> {
    let original: Termios = rustix::termios::tcgetattr(&file).map_err(|_| unavailable())?;
    let terminal: Terminal = Terminal { file, original };
    let mut quiet: Termios = terminal.original.clone();
    quiet
        .local_modes
        .remove(LocalModes::ECHO | LocalModes::ECHONL);
    rustix::termios::tcsetattr(&terminal.file, OptionalActions::Flush, &quiet)
        .map_err(|_| unavailable())?;
    let mut output: &File = &terminal.file;
    write!(output, "Credential (hidden): ")
        .and_then(|()| output.flush())
        .map_err(crate::storage::io_error)?;
    let input: AsyncFd<File> = AsyncFd::new(
        terminal
            .file
            .try_clone()
            .map_err(crate::storage::io_error)?,
    )
    .map_err(crate::storage::io_error)?;
    let value: String = operation.run(receive(&input)).await?;
    drop(terminal);
    Ok(Secret::new(value))
}

async fn receive(input: &AsyncFd<File>) -> Result<String, Error> {
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let mut ready: AsyncFdReadyGuard<'_, File> =
            input.readable().await.map_err(crate::storage::io_error)?;
        let mut buffer: [u8; 1024] = [0; 1024];
        let size: usize = match ready
            .try_io(|fd| rustix::io::read(fd.get_ref(), &mut buffer).map_err(std::io::Error::from))
        {
            Ok(result) => result.map_err(crate::storage::io_error)?,
            Err(_) => continue,
        };
        // Linux canonical input truncates longer lines; reject the boundary instead of saving a truncated secret.
        if size == 0 || bytes.len() + size > 4095 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&buffer[..size]);
        if let Some(end) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.truncate(end);
            let value: String = String::from_utf8(bytes).map_err(|_| unavailable())?;
            return super::configure::validate_secret(value);
        }
    }
}

fn unavailable() -> Error {
    Error::new(
        ErrorKind::Authentication,
        "credential prompt requires a controlling terminal and a nonempty single-line credential; use an environment reference for automation",
    )
}

#[cfg(test)]
#[path = "prompt/tests.rs"]
mod tests;
