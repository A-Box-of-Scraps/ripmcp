mod container;
mod process;

use crate::cli::Guard;
use crate::error::{Error, ErrorKind};
use crate::storage::Directory;
use rustix::fd::OwnedFd;
use rustix::process::{Pid, PidfdFlags};
use std::fs::File;
use tokio::io::unix::AsyncFd;

pub(super) fn check_support() -> Result<(), Error> {
    rustix::process::pidfd_open(rustix::process::getpid(), PidfdFlags::empty())
        .map(drop)
        .map_err(|_| {
            Error::new(
                ErrorKind::Unsupported,
                "local lifecycle requires Linux pidfd support",
            )
        })
}

pub fn run(guard: Guard) -> Result<(), Error> {
    rustix::process::umask(rustix::fs::Mode::RWXG | rustix::fs::Mode::RWXO);
    rustix::process::set_child_subreaper(Some(rustix::process::getpid()))
        .map_err(crate::storage::io_error)?;
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(supervise(guard))
}

async fn supervise(guard: Guard) -> Result<(), Error> {
    validate(&guard)?;
    // Do not inherit SIGCHLD=SIG_IGN: group ownership requires unreaped children.
    let _children: tokio::signal::unix::Signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())
            .map_err(crate::storage::io_error)?;
    let parent: Pid = Pid::from_raw(guard.parent as i32).ok_or_else(super::invalid)?;
    if rustix::process::getppid() != Some(parent) {
        return Err(super::unavailable());
    }
    let parent: AsyncFd<OwnedFd> = pidfd(parent)?;
    if Pid::as_raw(rustix::process::getppid()) != guard.parent as i32 {
        return Err(super::unavailable());
    }
    let directory: Directory =
        Directory::open(&guard.runtime, true, true)?.ok_or_else(super::invalid)?;
    let bytes: Vec<u8> = directory
        .read("supervisor.json", true)?
        .ok_or_else(super::invalid)?;
    let owner: super::endpoint::Record =
        serde_json::from_slice(&bytes).map_err(|_| super::invalid())?;
    if owner.pid != guard.parent
        || owner.protocol != super::wire::VERSION
        || owner.build != env!("CARGO_PKG_VERSION")
    {
        return Err(super::invalid());
    }
    let state: Directory = Directory::open(&guard.state, true, true)?.ok_or_else(super::invalid)?;
    let lock: File = state
        .file(
            &format!(".instance-{}.lock", guard.lease),
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CREATE,
            true,
        )?
        .ok_or_else(super::invalid)?;
    let mut term: tokio::signal::unix::Signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(crate::storage::io_error)?;
    let mut interrupt: tokio::signal::unix::Signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map_err(crate::storage::io_error)?;
    // The manager rechecks policy after waiting; do not defer execution again here.
    lock.try_lock().map_err(|_| super::unavailable())?;
    if Pid::as_raw(rustix::process::getppid()) != guard.parent as i32 {
        return Err(super::unavailable());
    }
    if state
        .read(&format!(".instance-{}.failed", guard.lease), true)?
        .is_some()
    {
        return Err(Error::new(
            ErrorKind::PartialFailure,
            "owned resource cleanup requires recovery",
        ));
    }
    let mut child: process::OwnedChild = process::OwnedChild::spawn(&guard, &state, &lock)?;
    tokio::select! {
        _ = child.exited() => (),
        _ = parent.readable() => (),
        _ = term.recv() => (),
        _ = interrupt.recv() => (),
    }
    let status: Result<std::process::ExitStatus, Error> = child.terminate().await;
    let result: Result<(), Error> = container::cleanup(&guard, &state).await;
    process::reap_descendants().await;
    result?;
    if guard.check_exit && !status?.success() {
        return Err(Error::new(
            ErrorKind::Connection,
            "runtime preparation failed",
        ));
    }
    Ok(())
}

fn validate(guard: &Guard) -> Result<(), Error> {
    let _: crate::ownership::InstallationId = guard
        .installation
        .clone()
        .try_into()
        .map_err(|_| super::invalid())?;
    if guard.revision.len() != 64 || !guard.revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(super::invalid());
    }
    if guard
        .container_env
        .iter()
        .any(|name| !crate::config::schema::environment_name(name))
    {
        return Err(super::invalid());
    }
    if guard.lease.len() != 64
        || !guard.lease.bytes().all(|byte| byte.is_ascii_hexdigit())
        || guard.command.is_empty()
        || !std::path::Path::new(&guard.command[0]).is_absolute()
        || guard.parent == 0
        || guard.parent > i32::MAX as u32
    {
        return Err(super::invalid());
    }
    if let Some(container) = &guard.container {
        let _: crate::ownership::InstallationId =
            container.clone().try_into().map_err(|_| super::invalid())?;
    }
    Ok(())
}

fn pidfd(pid: Pid) -> Result<AsyncFd<OwnedFd>, Error> {
    let fd: OwnedFd = rustix::process::pidfd_open(pid, PidfdFlags::empty()).map_err(|_| {
        Error::new(
            ErrorKind::Unsupported,
            "local lifecycle requires Linux pidfd support",
        )
    })?;
    AsyncFd::new(fd).map_err(crate::storage::io_error)
}
