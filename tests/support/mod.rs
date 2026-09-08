use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub struct Sandbox {
    pub root: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
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
            "XDG_RUNTIME_DIR",
        ] {
            command.env(key, self.root.path().join(key));
        }
        command.args(args).output().unwrap()
    }
}
