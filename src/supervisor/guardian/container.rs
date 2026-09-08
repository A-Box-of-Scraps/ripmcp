use crate::cli::Guard;
use crate::error::{Error, ErrorKind};
use crate::storage::Directory;
use std::fs::File;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::AsyncReadExt;

pub(super) fn configure(
    command: &mut Command,
    guard: &Guard,
    directory: &Directory,
) -> Result<(), Error> {
    let Some(nonce): Option<&String> = guard.container.as_ref() else {
        return Ok(());
    };
    let name: String = format!("ripmcp-{nonce}");
    let label: String = format!("io.ripmcp.owner={nonce}");
    let record: Vec<u8> = serde_json::to_vec(
        &serde_json::json!({"schema_version": 1, "container": name, "owner": nonce, "runtime": std::path::Path::new(&guard.command[0]), "installation": guard.installation, "revision": guard.revision}),
    )
    .map_err(crate::storage::io_error)?;
    let mut file: File = directory
        .file(
            &format!(".instance-{}.failed", guard.lease),
            rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::CREATE | rustix::fs::OFlags::EXCL,
            true,
        )?
        .ok_or_else(super::super::invalid)?;
    file.write_all(&record).map_err(crate::storage::io_error)?;
    file.sync_all().map_err(crate::storage::io_error)?;
    directory.sync()?;
    command.args([
        "run", "--rm", "--init", "-i", "--name", &name, "--label", &label,
    ]);
    for name in &guard.container_env {
        command.args(["--env", name]);
    }
    Ok(())
}

pub(super) async fn cleanup(guard: &Guard, directory: &Directory) -> Result<(), Error> {
    let Some(nonce): Option<&String> = guard.container.as_ref() else {
        return Ok(());
    };
    let label: String = format!("label=io.ripmcp.owner={nonce}");
    let name: String = format!("name=ripmcp-{nonce}");
    let output: String = invoke(
        guard,
        &[
            "container",
            "ls",
            "--all",
            "--quiet",
            "--no-trunc",
            "--filter",
            &label,
            "--filter",
            &name,
        ],
    )
    .await?;
    let id: &str = output.trim();
    if !id.is_empty() {
        if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(incomplete());
        }
        let expected: String = format!("{id} /ripmcp-{nonce} {nonce}");
        let identity: String = invoke(
            guard,
            &[
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.Id}} {{.Name}} {{index .Config.Labels \"io.ripmcp.owner\"}}",
                id,
            ],
        )
        .await?;
        if identity.trim() != expected {
            return Err(incomplete());
        }
        let _: String = invoke(guard, &["rm", "--force", id]).await?;
    }
    directory.remove(&format!(".instance-{}.failed", guard.lease))?;
    directory.sync()
}

async fn invoke(guard: &Guard, args: &[&str]) -> Result<String, Error> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut child: tokio::process::Child = tokio::process::Command::new(&guard.command[0])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| incomplete())?;
        let stdout: tokio::process::ChildStdout = child.stdout.take().ok_or_else(incomplete)?;
        let mut bytes: Vec<u8> = Vec::new();
        stdout
            .take(4097)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| incomplete())?;
        if bytes.len() > 4096 || !child.wait().await.map_err(|_| incomplete())?.success() {
            return Err(incomplete());
        }
        String::from_utf8(bytes).map_err(|_| incomplete())
    })
    .await
    .map_err(|_| incomplete())?
}

fn incomplete() -> Error {
    Error::new(
        ErrorKind::PartialFailure,
        "owned container cleanup failed; recovery record retained",
    )
}
