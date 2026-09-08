use ripmcp::{
    deadline::Deadline,
    mcp::{CancellationToken, Operation},
    storage::{Location, Paths},
    supervisor::Connection,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::Duration,
};

pub struct Fixture {
    pub root: tempfile::TempDir,
    pub paths: Paths,
    pub env: BTreeMap<OsString, OsString>,
}
impl Fixture {
    pub fn new() -> Self {
        let root: tempfile::TempDir = tempfile::tempdir().unwrap();
        let mut env: BTreeMap<OsString, OsString> = [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
        ]
        .into_iter()
        .map(|name| (name.into(), root.path().join(name).into_os_string()))
        .collect();
        for directory in env.values() {
            fs::create_dir(directory).unwrap();
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        env.insert("PATH".into(), root.path().as_os_str().to_owned());
        let paths: Paths = Paths::new(env.clone());
        for name in ["npx", "npm", "node", "uvx", "docker"] {
            let path: PathBuf = root.path().join(name);
            fs::write(&path, include_str!("runtime.py")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { root, paths, env }
    }
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command: Command = Command::new(env!("CARGO_BIN_EXE_ripmcp"));
        command
            .env_clear()
            .envs(&self.env)
            .current_dir(self.root.path())
            .stdin(Stdio::null())
            .args(args);
        command
    }
    pub fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    pub fn ok(&self, args: &[&str]) -> Value {
        let output: Output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ripmcp::json::parse(&output.stdout).unwrap()
    }
    pub fn config(&self) -> PathBuf {
        self.paths
            .directory(Location::Config)
            .unwrap()
            .join("config.json")
    }
    pub fn import(&self, value: &Value) -> PathBuf {
        let path: PathBuf = self.root.path().join("import.json");
        fs::write(&path, value.to_string()).unwrap();
        path
    }
    pub fn log(&self) -> String {
        fs::read_to_string(self.root.path().join("events")).unwrap_or_default()
    }
    pub fn flag(&self, name: &str) {
        fs::write(self.root.path().join(name), "").unwrap();
    }
    pub fn wait(&self, name: &str) {
        for _ in 0..500 {
            if self.root.path().join(name).exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("fixture did not create {name}");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(self.shutdown());
    }
}
impl Fixture {
    async fn shutdown(&self) {
        let operation: Operation = Operation::new(
            Deadline::new(Duration::from_secs(10)),
            CancellationToken::new(),
        );
        if let Ok(Some(connection)) = Connection::existing(&self.paths, &operation).await {
            let _: Result<(), ripmcp::error::Error> = connection.shutdown(&operation).await;
        }
        for _ in 0..1000 {
            if !self
                .root
                .path()
                .join("XDG_RUNTIME_DIR/ripmcp/supervisor.sock")
                .exists()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
