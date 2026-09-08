use ripmcp::deadline::Deadline;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::mcp::{CancellationToken, Operation};
use ripmcp::storage::Paths;
use ripmcp::supervisor::Connection;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

#[path = "supervisor/ipc.rs"]
mod ipc;

fn operation() -> Operation {
    Operation::new(
        Deadline::new(Duration::from_secs(5)),
        CancellationToken::new(),
    )
}

fn environment(root: &Path) -> BTreeMap<OsString, OsString> {
    [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ]
    .into_iter()
    .map(|key| (OsString::from(key), root.join(key).into_os_string()))
    .collect()
}

struct Fixture {
    root: TempDir,
    paths: Paths,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        ipc::shutdown_best_effort(self);
    }
}

impl Fixture {
    fn new() -> Self {
        let root: TempDir = tempfile::tempdir().unwrap();
        let runtime: PathBuf = root.path().join("XDG_RUNTIME_DIR");
        fs::create_dir(&runtime).unwrap();
        fs::set_permissions(runtime, fs::Permissions::from_mode(0o700)).unwrap();
        let paths: Paths = Paths::new(environment(root.path()));
        Self { root, paths }
    }

    fn runtime(&self) -> PathBuf {
        self.root.path().join("XDG_RUNTIME_DIR/ripmcp")
    }

    async fn ensure(&self) -> Connection {
        Connection::ensure(
            &self.paths,
            Path::new(env!("CARGO_BIN_EXE_ripmcp")),
            &operation(),
        )
        .await
        .unwrap()
    }

    async fn stop(&self) {
        Connection::existing(&self.paths, &operation())
            .await
            .unwrap()
            .unwrap()
            .shutdown(&operation())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.runtime().join("supervisor.sock").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    fn daemon(&self) -> Child {
        Command::new(env!("CARGO_BIN_EXE_ripmcp"))
            .arg("__supervisor")
            .env_clear()
            .envs(environment(self.root.path()))
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    async fn wait_ready(&self) {
        loop {
            if let Some(connection) = Connection::existing(&self.paths, &operation())
                .await
                .unwrap()
            {
                connection.ping(&operation()).await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[tokio::test]
async fn passive_connection_creates_nothing() {
    let fixture: Fixture = Fixture::new();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!fixture.runtime().exists());
}

#[tokio::test]
async fn bootstrap_is_persistent_and_reuses_one_endpoint() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let before: Vec<u8> = fs::read(fixture.runtime().join("supervisor.json")).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let after: Vec<u8> = fs::read(fixture.runtime().join("supervisor.json")).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        fs::metadata(fixture.runtime().join("supervisor.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fixture.stop().await;
}

#[tokio::test]
async fn bootstrap_helper() {
    let Some(root): Option<OsString> = std::env::var_os("RIPMCP_BOOTSTRAP_TEST_ROOT") else {
        return;
    };
    let paths: Paths = Paths::new(environment(Path::new(&root)));
    let connection: Connection = Connection::ensure(
        &paths,
        Path::new(env!("CARGO_BIN_EXE_ripmcp")),
        &operation(),
    )
    .await
    .unwrap();
    connection.ping(&operation()).await.unwrap();
}

#[tokio::test]
async fn concurrent_processes_bootstrap_once() {
    let fixture: Fixture = Fixture::new();
    let mut children: Vec<Child> = (0..8)
        .map(|_| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "bootstrap_helper", "--nocapture"])
                .env("RIPMCP_BOOTSTRAP_TEST_ROOT", fixture.root.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let mut duplicate: Child = fixture.daemon();
    assert_eq!(duplicate.wait().unwrap().code(), Some(4));
    fixture.stop().await;
}

#[tokio::test]
async fn an_arbitrary_socket_is_preserved_and_rejected() {
    let fixture: Fixture = Fixture::new();
    fixture.paths.runtime(true).unwrap();
    let path: PathBuf = fixture.runtime().join("supervisor.sock");
    let listener: std::os::unix::net::UnixListener =
        std::os::unix::net::UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    let result: Result<Connection, Error> = Connection::ensure(
        &fixture.paths,
        Path::new(env!("CARGO_BIN_EXE_ripmcp")),
        &operation(),
    )
    .await;
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::Configuration,
            ..
        })
    ));
    assert!(path.exists());
}

#[tokio::test]
async fn symlinks_and_unsafe_socket_permissions_are_rejected() {
    let fixture: Fixture = Fixture::new();
    fixture.paths.runtime(true).unwrap();
    let path: PathBuf = fixture.runtime().join("supervisor.sock");
    std::os::unix::fs::symlink("/dev/null", &path).unwrap();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .is_err()
    );
    fs::remove_file(&path).unwrap();
    let _listener: std::os::unix::net::UnixListener =
        std::os::unix::net::UnixListener::bind(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn crash_recovery_replaces_only_a_recorded_stale_socket() {
    let fixture: Fixture = Fixture::new();
    let mut daemon: Child = fixture.daemon();
    tokio::time::timeout(Duration::from_secs(5), fixture.wait_ready())
        .await
        .unwrap();
    let before: Vec<u8> = fs::read(fixture.runtime().join("supervisor.json")).unwrap();
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .unwrap()
            .is_none()
    );
    fixture.ensure().await.ping(&operation()).await.unwrap();
    assert_ne!(
        before,
        fs::read(fixture.runtime().join("supervisor.json")).unwrap()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_different_storage_context_cannot_reuse_the_daemon() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let mut alternate: BTreeMap<OsString, OsString> = environment(fixture.root.path());
    alternate.insert(
        OsString::from("XDG_STATE_HOME"),
        fixture.root.path().join("different").into_os_string(),
    );
    assert!(
        Connection::existing(&Paths::new(alternate), &operation())
            .await
            .is_err()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn bootstrap_lock_wait_consumes_the_operation_deadline() {
    let fixture: Fixture = Fixture::new();
    fixture.paths.runtime(true).unwrap();
    let lock: fs::File = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(fixture.runtime().join(".bootstrap.lock"))
        .unwrap();
    lock.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    lock.lock().unwrap();
    let operation: Operation = Operation::new(
        Deadline::new(Duration::from_millis(20)),
        CancellationToken::new(),
    );
    let result: Result<Connection, Error> = Connection::ensure(
        &fixture.paths,
        Path::new(env!("CARGO_BIN_EXE_ripmcp")),
        &operation,
    )
    .await;
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::Timeout,
            ..
        })
    ));
    assert!(!fixture.runtime().join("supervisor.sock").exists());
}

#[tokio::test]
async fn changing_runtime_directory_cannot_create_a_second_supervisor_for_the_same_state() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let runtime: TempDir = tempfile::tempdir().unwrap();
    fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut alternate: BTreeMap<OsString, OsString> = environment(fixture.root.path());
    alternate.insert(
        OsString::from("XDG_RUNTIME_DIR"),
        runtime.path().as_os_str().to_owned(),
    );
    let result: Result<Connection, Error> = Connection::ensure(
        &Paths::new(alternate),
        Path::new(env!("CARGO_BIN_EXE_ripmcp")),
        &operation(),
    )
    .await;
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::Connection,
            ..
        })
    ));
    assert!(!runtime.path().join("ripmcp/supervisor.sock").exists());
    fixture.ensure().await.ping(&operation()).await.unwrap();
    fixture.stop().await;
}
