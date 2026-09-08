use super::{
    StderrLog, StdioOptions,
    registry::{Registry, WriteMessage},
};
use crate::error::Error;
use crate::mcp::{connection_error, limit_error, protocol_error};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(super) fn spawn(
    options: StdioOptions,
    receiver: mpsc::Receiver<WriteMessage>,
    registry: Arc<Registry>,
    stop: CancellationToken,
    logs: Arc<Mutex<StderrLog>>,
    limit: usize,
) -> Result<JoinHandle<()>, Error> {
    let grace: Duration = options.shutdown_grace;
    let termination: Duration = options.termination_grace;
    let mut command: Command = Command::new(options.program);
    command
        .args(options.args)
        .env_clear()
        .envs(options.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = options.cwd {
        command.current_dir(cwd);
    }
    let mut child: Child = command.spawn().map_err(|_| connection_error())?;
    let stdin: ChildStdin = child.stdin.take().ok_or_else(connection_error)?;
    let stdout: ChildStdout = child.stdout.take().ok_or_else(connection_error)?;
    let stderr: ChildStderr = child.stderr.take().ok_or_else(connection_error)?;
    Ok(tokio::spawn(async move {
        let mut reader: JoinHandle<Result<(), Error>> =
            tokio::spawn(read(stdout, registry.clone(), limit));
        let mut writer: JoinHandle<Result<(), Error>> = tokio::spawn(write(stdin, receiver));
        let logger: JoinHandle<()> = tokio::spawn(log(stderr, logs));
        let error: Error = tokio::select! {
            result = &mut reader => task_error(result),
            result = &mut writer => task_error(result),
            _ = child.wait() => drain(&mut reader).await,
            () = stop.cancelled() => connection_error(),
        };
        registry.close(error);
        reader.abort();
        writer.abort();
        // Aborting the writer closes stdin before waiting for graceful child exit.
        if !writer.is_finished() {
            let _: Result<Result<(), Error>, tokio::task::JoinError> = writer.await;
        }
        terminate(&mut child, grace, termination).await;
        logger.abort();
    }))
}

fn task_error(result: Result<Result<(), Error>, tokio::task::JoinError>) -> Error {
    result
        .ok()
        .and_then(Result::err)
        .unwrap_or_else(connection_error)
}

async fn drain(reader: &mut JoinHandle<Result<(), Error>>) -> Error {
    // A process can exit while its final response is still buffered in stdout.
    tokio::time::timeout(Duration::from_millis(250), reader)
        .await
        .map(task_error)
        .unwrap_or_else(|_| connection_error())
}

async fn read(stdout: ChildStdout, registry: Arc<Registry>, limit: usize) -> Result<(), Error> {
    let mut reader: BufReader<ChildStdout> = BufReader::new(stdout);
    let mut frame: Vec<u8> = Vec::new();
    loop {
        let buffer: &[u8] = reader.fill_buf().await.map_err(|_| connection_error())?;
        if buffer.is_empty() {
            return Err(if frame.is_empty() {
                connection_error()
            } else {
                protocol_error()
            });
        }
        let newline: Option<usize> = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(buffer.len(), |position| position + 1);
        let payload = count - usize::from(newline.is_some());
        if payload > limit.saturating_sub(frame.len()) {
            return Err(limit_error());
        }
        frame.extend_from_slice(&buffer[..payload]);
        reader.consume(count);
        if newline.is_some() {
            registry.receive(&frame)?;
            frame.clear();
        }
    }
}

async fn write(
    mut stdin: ChildStdin,
    mut receiver: mpsc::Receiver<WriteMessage>,
) -> Result<(), Error> {
    while let Some(message) = receiver.recv().await {
        stdin
            .write_all(&message.bytes)
            .await
            .map_err(|_| connection_error())?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|_| connection_error())?;
        stdin.flush().await.map_err(|_| connection_error())?;
        drop(message.permit);
    }
    Ok(())
}

async fn log(mut stderr: ChildStderr, logs: Arc<Mutex<StderrLog>>) {
    let mut buffer: [u8; 4096] = [0; 4096];
    while let Ok(count) = stderr.read(&mut buffer).await {
        if count == 0 {
            break;
        }
        let mut log: MutexGuard<'_, StderrLog> = logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        log.redacted_bytes = log.redacted_bytes.saturating_add(count as u64);
        log.redacted_chunks = log.redacted_chunks.saturating_add(1);
        buffer[..count].fill(0);
    }
}

async fn terminate(child: &mut Child, grace: Duration, termination: Duration) {
    if tokio::time::timeout(grace, child.wait()).await.is_ok() {
        return;
    }
    if let Some(pid) = child
        .id()
        .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
    {
        let _: std::io::Result<()> =
            rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(Into::into);
    }
    if tokio::time::timeout(termination, child.wait())
        .await
        .is_err()
    {
        let _: std::io::Result<()> = child.kill().await;
    }
}
