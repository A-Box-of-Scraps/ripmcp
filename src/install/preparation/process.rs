use super::Context;
use crate::{
    error::{Error, ErrorKind},
    storage::{Directory, Location},
};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdout, Command},
};

pub(super) async fn run(
    context: &Context<'_>,
    executable: &Path,
    args: &[&str],
) -> Result<String, Error> {
    context.operation.remaining()?;
    let runtime: Directory = context
        .paths
        .runtime(true)?
        .ok_or_else(crate::install::invalid)?;
    let root: std::path::PathBuf =
        std::fs::read_link(runtime.descriptor_path()).map_err(crate::storage::io_error)?;
    let key: String = crate::config::digest(context.installation.id().as_str().as_bytes());
    let mut command: Command =
        Command::new(std::env::current_exe().map_err(crate::storage::io_error)?);
    command
        .args([
            "__guard",
            &key,
            &std::process::id().to_string(),
            "--check-exit",
            "--runtime",
        ])
        .arg(root)
        .arg("--state")
        .arg(context.paths.directory(Location::State)?)
        .args([
            "--installation",
            context.installation.id().as_str(),
            "--revision",
            &key,
            "--",
        ])
        .arg(executable)
        .args(args)
        .env_clear()
        .envs(
            context
                .environment
                .iter()
                .filter(|(key, _)| crate::supervisor::launch::BASE_ENV.contains(&key.as_str())),
        )
        .env("UV_PYTHON_DOWNLOADS", "never")
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    context.target.check()?;
    context.operation.remaining()?;
    let mut child: Child = command.spawn().map_err(|_| failed())?;
    let stdout: ChildStdout = child.stdout.take().ok_or_else(failed)?;
    let mut bytes: Vec<u8> = Vec::new();
    let result: Result<(), Error> = context
        .operation
        .run(async {
            stdout
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| failed())?;
            if bytes.len() > 1024 * 1024 || !child.wait().await.map_err(|_| failed())?.success() {
                return Err(failed());
            }
            Ok(())
        })
        .await;
    if result.is_err() {
        terminate(&mut child).await;
    }
    result?;
    String::from_utf8(bytes).map_err(|_| failed())
}

async fn terminate(child: &mut Child) {
    if let Some(pid) = child
        .id()
        .and_then(|id| rustix::process::Pid::from_raw(id as i32))
    {
        let _: Result<(), rustix::io::Errno> =
            rustix::process::kill_process(pid, rustix::process::Signal::TERM);
    }
    if tokio::time::timeout(Duration::from_secs(3), child.wait())
        .await
        .is_err()
    {
        let _: std::io::Result<()> = child.kill().await;
    }
}
fn failed() -> Error {
    Error::new(
        ErrorKind::Connection,
        "runtime preparation failed; requested version may be unavailable; shared caches and images were preserved",
    )
}
