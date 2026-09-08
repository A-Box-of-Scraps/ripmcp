use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub struct Sandbox {
    pub root: TempDir,
    runtime: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
            runtime: tempfile::tempdir().unwrap(),
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        let mut command: Command = Command::new(env!("CARGO_BIN_EXE_ripmcp"));
        command
            .env_clear()
            .current_dir(self.root.path())
            .stdin(Stdio::null());
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
        ] {
            command.env(key, self.root.path().join(key));
        }
        command
            .env("XDG_RUNTIME_DIR", self.runtime.path())
            .args(args)
            .output()
            .unwrap()
    }
}
