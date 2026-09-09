use ripmcp::config::{Source, schema::Runtime};
use ripmcp::ownership::{Installation, Origin, OwnershipStore};
use ripmcp::storage::{Location, Paths};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub(super) struct Fixture {
    pub root: TempDir,
    pub paths: Paths,
    pub env: BTreeMap<OsString, OsString>,
    pub executable: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root: TempDir = tempfile::tempdir().unwrap();
        let env: BTreeMap<OsString, OsString> = [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
        ]
        .into_iter()
        .map(|key| (key.into(), root.path().join(key).into_os_string()))
        .collect();
        let paths: Paths = Paths::new(env.clone());
        fs::create_dir(root.path().join("XDG_RUNTIME_DIR")).unwrap();
        fs::set_permissions(
            root.path().join("XDG_RUNTIME_DIR"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let executable: PathBuf = root.path().join("npx");
        fs::write(
            &executable,
            include_str!("../support/fixtures/lifecycle.py"),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            env,
            paths,
            executable,
        }
    }

    pub fn configure(&self, mode: &str) -> Value {
        let data: PathBuf = self.root.path().join("server");
        fs::create_dir_all(&data).unwrap();
        let value: Value = json!({"schema_version": 1, "servers": {"s": {"definition": {
            "kind": "local", "runtime": "npx", "package": "fixture", "transport": "stdio", "args": [mode, data]
        }}}});
        self.write_user(&value);
        let scope: Source = Source::User {
            config: self.config().canonicalize().unwrap(),
        };
        self.install(scope, Runtime::Npx, "fixture@1.2.3");
        value
    }

    pub fn install(&self, scope: Source, runtime: Runtime, resolved: &str) {
        let entry: Installation = Installation::new(
            scope,
            "s".to_owned(),
            Origin::Local {
                runtime,
                requested: "fixture".to_owned(),
                resolved: Some(resolved.to_owned()),
                executable: Some(self.executable.clone()),
            },
        )
        .unwrap();
        OwnershipStore::new(&self.paths)
            .unwrap()
            .update(|journal| {
                journal
                    .installations
                    .insert(entry.id().as_str().to_owned(), entry);
                Ok(())
            })
            .unwrap();
    }

    pub fn config(&self) -> PathBuf {
        self.paths
            .directory(Location::Config)
            .unwrap()
            .join("config.json")
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.path().join("XDG_RUNTIME_DIR/ripmcp")
    }
    pub fn write_user(&self, value: &Value) {
        write(&self.config(), value);
    }
    pub fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command: Command = Command::new(env!("CARGO_BIN_EXE_ripmcp"));
        command
            .env_clear()
            .envs(&self.env)
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null());
        command
    }
    pub fn run(&self, args: &[&str]) -> Output {
        self.command(self.root.path(), args).output().unwrap()
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
    pub fn pid(&self) -> u32 {
        fs::read_to_string(self.root.path().join("server/pid"))
            .unwrap()
            .parse()
            .unwrap()
    }
    pub fn launches(&self) -> usize {
        fs::read_to_string(self.root.path().join("server/launches"))
            .unwrap()
            .lines()
            .count()
    }
}

pub(super) fn write(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

impl Drop for Fixture {
    fn drop(&mut self) {
        shutdown(self);
    }
}

fn shutdown(fixture: &Fixture) {
    use std::io::Write;
    let Ok(mut stream): std::io::Result<std::os::unix::net::UnixStream> =
        std::os::unix::net::UnixStream::connect(fixture.runtime().join("supervisor.sock"))
    else {
        return;
    };
    let Ok(bytes): std::io::Result<Vec<u8>> = fs::read(fixture.runtime().join("supervisor.json"))
    else {
        return;
    };
    let Ok(record): Result<Value, serde_json::Error> = ripmcp::json::parse(&bytes) else {
        return;
    };
    let hello: Value = json!({"protocol": 4, "build": env!("CARGO_PKG_VERSION"), "nonce": record["nonce"], "context": record["context"]});
    let _: std::io::Result<()> = stream.set_write_timeout(Some(std::time::Duration::from_secs(1)));
    for value in [hello, json!("shutdown")] {
        let bytes: Vec<u8> = serde_json::to_vec(&value).unwrap();
        let _: std::io::Result<()> = stream.write_all(&(bytes.len() as u32).to_be_bytes());
        let _: std::io::Result<()> = stream.write_all(&bytes);
    }
    for _ in 0..1000 {
        if !fixture.runtime().join("supervisor.sock").exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
