use super::{container, pidfd};
use crate::cli::Guard;
use crate::error::Error;
use crate::storage::Directory;
use rustix::fd::OwnedFd;
use rustix::process::{Pid, Signal};
use std::fs::File;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::unix::AsyncFd;

pub(super) struct OwnedChild {
    child: Child,
    pid: Pid,
    readiness: AsyncFd<OwnedFd>,
    reaped: bool,
}

impl OwnedChild {
    pub fn spawn(guard: &Guard, directory: &Directory, lock: &File) -> Result<Self, Error> {
        let mut command: Command = Command::new(&guard.command[0]);
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .process_group(0);
        container::configure(&mut command, guard, directory)?;
        command.args(&guard.command[1..]);
        let parent: Pid = rustix::process::getpid();
        let lease = lock.as_raw_fd();
        // Keep the ownership lease inherited by descendants if the guard crashes.
        unsafe {
            command.pre_exec(move || {
                rustix::process::set_parent_process_death_signal(Some(Signal::KILL))?;
                if rustix::process::getppid() != Some(parent) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                rustix::io::fcntl_setfd(BorrowedFd::borrow_raw(lease), rustix::io::FdFlags::empty())
                    .map_err(Into::into)
            });
        }
        let mut child: Child = command.spawn().map_err(crate::storage::io_error)?;
        let pid: Pid = Pid::from_raw(child.id() as i32).ok_or_else(super::super::invalid)?;
        let readiness: AsyncFd<OwnedFd> = match pidfd(pid) {
            Ok(fd) => fd,
            Err(error) => {
                let _: Result<(), rustix::io::Errno> =
                    rustix::process::kill_process_group(pid, Signal::KILL);
                let _: std::io::Result<std::process::ExitStatus> = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            child,
            pid,
            readiness,
            reaped: false,
        })
    }

    pub async fn exited(&self) {
        let _: std::io::Result<tokio::io::unix::AsyncFdReadyGuard<'_, OwnedFd>> =
            self.readiness.readable().await;
    }

    pub async fn terminate(&mut self) {
        let _: Result<(), tokio::time::error::Elapsed> =
            tokio::time::timeout(Duration::from_millis(250), self.exited()).await;
        self.signal(Signal::TERM);
        tokio::time::sleep(Duration::from_millis(250)).await;
        self.signal(Signal::KILL);
        self.exited().await;
        let _: std::io::Result<std::process::ExitStatus> = self.child.wait();
        self.reaped = true;
        reap_descendants().await;
    }

    fn signal(&self, signal: Signal) {
        // The direct child remains unreaped until all group signals are sent.
        let _: Result<(), rustix::io::Errno> =
            rustix::process::kill_process_group(self.pid, signal);
    }
}

pub(super) async fn reap_descendants() {
    let deadline: tokio::time::Instant = tokio::time::Instant::now() + Duration::from_millis(250);
    loop {
        match rustix::process::waitpid(None, rustix::process::WaitOptions::NOHANG) {
            Ok(Some(_)) => (),
            Err(_) => return,
            Ok(None) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await
            }
            Ok(None) => return,
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            self.signal(Signal::KILL);
            let _: std::io::Result<std::process::ExitStatus> = self.child.wait();
        }
    }
}
