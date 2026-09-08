use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub struct Script {
    root: TempDir,
    executable: PathBuf,
}

impl Script {
    pub fn new(name: &str, body: &str) -> Self {
        assert!(matches!(name, "npx" | "uvx" | "docker" | "stdio"));
        let root: TempDir = tempfile::tempdir().unwrap();
        let executable: PathBuf = root.path().join(name);
        fs::write(&executable, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root, executable }
    }

    pub fn spawn(&self, args: &[&str]) -> Process {
        let mut command: Command = Command::new(&self.executable);
        command.env_clear().current_dir(self.root.path()).args(args);
        for name in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
        ] {
            command.env(name, self.root.path().join(name));
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Process {
            child: Some(command.spawn().unwrap()),
        }
    }
}

pub struct Process {
    child: Option<Child>,
}

impl Process {
    pub fn child(&mut self) -> &mut Child {
        self.child.as_mut().unwrap()
    }

    pub fn finish(mut self, timeout: Duration) -> io::Result<Output> {
        let started: Instant = Instant::now();
        loop {
            if self.child().try_wait()?.is_some() {
                return self.child.take().unwrap().wait_with_output();
            }
            if started.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture process timed out",
                ));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _: io::Result<()> = child.kill();
            let _: io::Result<std::process::ExitStatus> = child.wait();
        }
    }
}
