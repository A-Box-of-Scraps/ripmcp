#[allow(dead_code)]
#[path = "install/fixture.rs"]
mod fixture;
use fixture::Fixture;
use ripmcp::ownership::{
    CleanupState, Executable, Filesystem, Origin, Ownership, OwnershipStore, Resource,
    ResourceIdentity, ResourceKind,
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Sandbox {
    fixture: Fixture,
    executable: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let fixture: Fixture = Fixture::new();
        let executable: PathBuf = fixture.root.path().join("ripmcp-copy");
        fs::copy(env!("CARGO_BIN_EXE_ripmcp"), &executable).unwrap();
        let sandbox: Self = Self {
            fixture,
            executable,
        };
        assert!(sandbox.run(&["--version"]).status.success());
        sandbox
    }
    fn run(&self, args: &[&str]) -> Output {
        let started: std::time::Instant = std::time::Instant::now();
        loop {
            match self.command(args).output() {
                Ok(output) => return output,
                Err(error)
                    if error.raw_os_error() == Some(26)
                        && started.elapsed() < std::time::Duration::from_secs(5) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(error) => panic!("sandbox command failed: {error}"),
            }
        }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command: Command = Command::new(&self.executable);
        command
            .env_clear()
            .envs(&self.fixture.env)
            .current_dir(self.fixture.root.path())
            .stdin(Stdio::null())
            .args(args);
        command
    }
    fn install(&self) {
        let output: Output = self.run(&["install", "s", "--npx", "fixture"]);
        assert!(output.status.success(), "{output:?}");
    }
    fn provenance(&self, executable: Executable) {
        OwnershipStore::new(&self.fixture.paths)
            .unwrap()
            .update(|journal| {
                journal.executable = executable;
                Ok(())
            })
            .unwrap();
    }
    fn standalone(&self) {
        self.provenance(
            Executable::installed_copy(&self.executable, self.fixture.root.path()).unwrap(),
        );
    }
    fn owned(&self) -> PathBuf {
        let path: PathBuf = self.fixture.root.path().join("owned-data");
        fs::write(&path, "exclusive").unwrap();
        OwnershipStore::new(&self.fixture.paths)
            .unwrap()
            .update(|journal| {
                journal
                    .installations
                    .values_mut()
                    .next()
                    .unwrap()
                    .resources
                    .push(Resource {
                        kind: ResourceKind::Data,
                        identity: ResourceIdentity::Path {
                            canonical_path: path.clone(),
                        },
                        origin: Origin::Standalone,
                        ownership: Ownership::Exclusive,
                        cleanup: CleanupState::Pending,
                        filesystem: Some(
                            Filesystem::created(&path, self.fixture.root.path()).unwrap(),
                        ),
                    });
                Ok(())
            })
            .unwrap();
        path
    }
}

#[test]
fn nonterminal_self_uninstall_does_not_create_state_or_remove_binary() {
    let sandbox: Sandbox = Sandbox::new();
    let output: Output = sandbox.run(&["--uninstall-everything"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        !sandbox
            .fixture
            .paths
            .directory(ripmcp::storage::Location::State)
            .unwrap()
            .exists()
    );
    assert!(sandbox.executable.exists());
}

#[test]
fn unknown_and_package_managed_binaries_are_preserved_with_retry_evidence() {
    for provenance in [
        Executable::Unknown,
        Executable::PackageManager {
            manager: "pacman".to_owned(),
        },
    ] {
        let sandbox: Sandbox = Sandbox::new();
        sandbox.install();
        let data: PathBuf = sandbox.owned();
        sandbox.provenance(provenance);
        let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
        assert_eq!(output.status.code(), Some(8), "{output:?}");
        assert!(!data.exists());
        assert!(sandbox.executable.exists());
        let state: PathBuf = sandbox
            .fixture
            .paths
            .directory(ripmcp::storage::Location::State)
            .unwrap();
        assert!(state.join("self-removal.json").exists());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!report["failures"].as_array().unwrap().is_empty());
        assert_eq!(
            sandbox
                .run(&["install", "new", "--npx", "fixture"])
                .status
                .code(),
            Some(8)
        );
        assert_eq!(
            sandbox.run(&["--uninstall-everything", "-y"]).status.code(),
            Some(8)
        );
    }
}

#[test]
fn copied_standalone_is_removed_only_after_resources_and_supervisor_stop() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    let data: PathBuf = sandbox.owned();
    let project: PathBuf = sandbox.fixture.root.path().join("project/.ripmcp");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("config.json"), "unrelated project bytes").unwrap();
    sandbox.standalone();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!data.exists());
    assert!(!sandbox.executable.exists());
    assert!(!sandbox.fixture.config().exists());
    let state: PathBuf = sandbox
        .fixture
        .paths
        .directory(ripmcp::storage::Location::State)
        .unwrap();
    assert!(!state.join("ownership.json").exists());
    assert!(!state.join("artifacts.json").exists());
    assert!(!state.join("self-removal.json").exists());
    assert!(PathBuf::from(env!("CARGO_BIN_EXE_ripmcp")).exists());
    assert!(project.join("config.json").exists());
    assert!(
        !sandbox
            .fixture
            .root
            .path()
            .join("XDG_RUNTIME_DIR/ripmcp/supervisor.sock")
            .exists()
    );
}

#[test]
fn resource_failure_preserves_executable_and_retry_can_finish() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    let data: PathBuf = sandbox.owned();
    sandbox.standalone();
    fs::rename(&data, data.with_extension("original")).unwrap();
    fs::write(&data, "foreign replacement").unwrap();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(sandbox.executable.exists());
    assert_eq!(fs::read(&data).unwrap(), b"foreign replacement");
    fs::remove_file(&data).unwrap();
    fs::rename(data.with_extension("original"), &data).unwrap();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!data.exists());
    assert!(!sandbox.executable.exists());
}

#[test]
fn executable_path_substitution_is_preserved() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    sandbox.standalone();
    fs::rename(
        &sandbox.executable,
        sandbox.executable.with_extension("original"),
    )
    .unwrap();
    fs::copy(env!("CARGO_BIN_EXE_ripmcp"), &sandbox.executable).unwrap();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(sandbox.executable.exists());
    fs::remove_file(&sandbox.executable).unwrap();
    fs::rename(
        sandbox.executable.with_extension("original"),
        &sandbox.executable,
    )
    .unwrap();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!sandbox.executable.exists());
}

#[test]
fn unavailable_credential_store_preserves_binary_and_retry_metadata() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    let path: PathBuf = sandbox.fixture.import(&serde_json::json!({"definition": {"kind": "remote", "url": "https://example.invalid/mcp", "transport": "streamable_http", "authentication": "oauth"}}));
    let output: Output = sandbox.run(&[
        "install",
        "oauth",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert!(output.status.success(), "{output:?}");
    sandbox.standalone();
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(sandbox.executable.exists());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["action"] == "remove_oauth_credentials")
    );
}

#[test]
fn self_removal_fences_concurrent_auto_start() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    sandbox.standalone();
    let child: std::process::Child = sandbox
        .command(&["--uninstall-everything", "-y"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    sandbox
        .fixture
        .wait("XDG_STATE_HOME/ripmcp/self-removal.json");
    let output: Output = sandbox.fixture.run(&["start", "s"]);
    assert!(!output.status.success());
    let output: Output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(sandbox.fixture.root.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!sandbox.executable.exists());
}

#[test]
fn declining_self_removal_has_no_persistent_side_effects() {
    use rustix::pty::{OpenptFlags, grantpt, ioctl_tiocgptpeer, openpt, unlockpt};
    use std::{
        fs::File,
        io::{Read, Write},
        time::{Duration, Instant},
    };
    let sandbox: Sandbox = Sandbox::new();
    let mut master: File = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)
        .map(File::from)
        .unwrap();
    rustix::fs::fcntl_setfl(&master, rustix::fs::OFlags::NONBLOCK).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let slave: File = ioctl_tiocgptpeer(
        &master,
        OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
    )
    .map(File::from)
    .unwrap();
    let child: std::process::Child = sandbox
        .command(&["--uninstall-everything"])
        .stdin(slave.try_clone().unwrap())
        .stderr(slave)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let started: Instant = Instant::now();
    let mut preview: Vec<u8> = Vec::new();
    while !preview.ends_with(b"[y/N] ") {
        let mut buffer: [u8; 8192] = [0; 8192];
        match master.read(&mut buffer) {
            Ok(count) => preview.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("terminal read failed: {error}"),
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    master.write_all(b"\n").unwrap();
    let output: Output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(130));
    assert!(sandbox.executable.exists());
    assert!(
        !sandbox
            .fixture
            .paths
            .directory(ripmcp::storage::Location::State)
            .unwrap()
            .exists()
    );
}

#[test]
fn only_recorded_installation_links_are_removed_without_following_targets() {
    let sandbox: Sandbox = Sandbox::new();
    sandbox.install();
    let external: PathBuf = sandbox.fixture.root.path().join("external-data");
    fs::write(&external, "preserved target").unwrap();
    let owned: PathBuf = sandbox.fixture.root.path().join("installed-link");
    let unknown: PathBuf = sandbox.fixture.root.path().join("untracked-link");
    std::os::unix::fs::symlink(&external, &owned).unwrap();
    std::os::unix::fs::symlink(&external, &unknown).unwrap();
    let mut provenance: Executable =
        Executable::installed_copy(&sandbox.executable, sandbox.fixture.root.path()).unwrap();
    provenance
        .record_created_link(&owned, sandbox.fixture.root.path())
        .unwrap();
    sandbox.provenance(provenance);
    let output: Output = sandbox.run(&["--uninstall-everything", "-y"]);
    assert!(output.status.success(), "{output:?}");
    assert!(fs::symlink_metadata(owned).is_err());
    assert!(fs::symlink_metadata(unknown).unwrap().is_symlink());
    assert_eq!(fs::read(external).unwrap(), b"preserved target");
    assert!(!sandbox.executable.exists());
}
