use super::fixture::Fixture;
use ripmcp::{
    config::Project,
    ownership::{Journal, OperationState, OwnershipStore, Registration},
    trust::TrustStore,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Output, Stdio},
    time::Duration,
};

#[test]
fn duplicate_target_is_rejected_before_runtime_and_existing_config_survives() {
    let fixture: Fixture = Fixture::new();
    let _: Value = fixture.ok(&["install", "s", "--npx", "fixture", "--skip-verify"]);
    let bytes: Vec<u8> = fs::read(fixture.config()).unwrap();
    let log: String = fixture.log();
    fixture.flag("fail");
    let output: Output = fixture.run(&["install", "s", "--npx", "different"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(fs::read(fixture.config()).unwrap(), bytes);
    assert_eq!(fixture.log(), log);
    let output: Output = fixture.run(&["install", "other", "--npx", "fixture"]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(fs::read(fixture.config()).unwrap(), bytes);
}

#[test]
fn verification_failure_rolls_back_and_container_failure_retains_recovery() {
    for runtime in ["--npx", "--docker"] {
        let fixture: Fixture = Fixture::new();
        fixture.flag("verify-fail");
        if runtime == "--docker" {
            fixture.flag("cleanup-fail");
        }
        let output: Output = fixture.run(&["install", "s", runtime, "fixture"]);
        assert!(!output.status.success());
        assert!(!fixture.config().exists());
        let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
        assert!(
            journal
                .installations
                .values()
                .all(|entry| entry.registration == Registration::Unregistered)
        );
        assert!(
            journal
                .operations
                .values()
                .all(|entry| matches!(entry.state, OperationState::RetryRequired))
        );
        assert!(!fixture.log().contains("tools/call"));
    }
}

#[test]
fn cancellation_and_timeout_reap_preparation_descendants() {
    for runtime in ["--npx", "--uvx", "--docker"] {
        let fixture: Fixture = Fixture::new();
        fixture.flag("stall");
        let child: Child = fixture
            .command(&["install", "s", runtime, "fixture", "--timeout", "5"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        fixture.wait("preparation_child");
        let pid: i32 = fs::read_to_string(fixture.root.path().join("preparation_child"))
            .unwrap()
            .parse()
            .unwrap();
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
            rustix::process::Signal::INT,
        )
        .unwrap();
        let output: Output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(130), "{output:?}");
        for _ in 0..500 {
            if !PathBuf::from(format!("/proc/{pid}")).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
        assert!(!fixture.config().exists());
    }
    let fixture: Fixture = Fixture::new();
    fixture.flag("stall");
    let output: Output = fixture.run(&["install", "s", "--npx", "fixture", "--timeout", "1"]);
    assert_eq!(output.status.code(), Some(9));
    assert!(!fixture.config().exists());
}

#[test]
fn concurrent_external_edits_are_not_overwritten() {
    let fixture: Fixture = Fixture::new();
    fixture.flag("verify-stall");
    let child: Child = fixture
        .command(&["install", "s", "--npx", "fixture", "--timeout", "3"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let replacement: &[u8] = b"{\"schema_version\":1,\"servers\":{}}";
    fs::write(fixture.config(), replacement).unwrap();
    let output: Output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(fixture.config()).unwrap(), replacement);
}

#[test]
fn project_trust_scope_shadowing_and_reapproval_are_enforced() {
    let fixture: Fixture = Fixture::new();
    let output: Output = fixture.run(&[
        "install",
        "s",
        "--npx",
        "fixture",
        "--project",
        "--skip-verify",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let root: PathBuf = fixture.root.path().join("project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        json!({"schema_version":1,"servers":{}}).to_string(),
    )
    .unwrap();
    let output: Output = fixture
        .command(&[
            "install",
            "s",
            "--npx",
            "fixture",
            "--project",
            "--skip-verify",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(fixture.log().is_empty());
    let project: Project = Project::discover(&root).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    let output: Output = fixture
        .command(&["install", "s", "--npx", "fixture", "--project"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let report: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(report["installation"]["project_reapproval_required"], true);
    let project: Project = Project::discover(&root).unwrap().unwrap();
    assert!(
        !TrustStore::new(&fixture.paths)
            .unwrap()
            .is_approved(&project)
            .unwrap()
    );
    let output: Output = fixture
        .command(&["start", "s"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    let output: Output = fixture
        .command(&[
            "install",
            "s",
            "--uvx",
            "fixture",
            "--user",
            "--skip-verify",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let report: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(report["installation"]["shadowed_by_trusted_project"], true);
}

#[test]
fn interrupted_config_commit_retains_retry_record_and_no_active_definition() {
    let fixture: Fixture = Fixture::new();
    fixture.flag("verify-pause");
    let child: Child = fixture
        .command(&["install", "s", "--npx", "fixture"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let pending: PathBuf = fixture
        .config()
        .parent()
        .unwrap()
        .join(".config.json.pending");
    fs::create_dir(&pending).unwrap();
    fixture.flag("release");
    let output: Output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(!fixture.config().exists());
    assert!(pending.is_dir());
    let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
    let entry: &ripmcp::ownership::Installation = journal.installations.values().next().unwrap();
    assert_eq!(entry.registration, Registration::Unregistered);
    assert!(entry.definition.is_some());
    assert!(entry.configuration_digest.is_some());
    assert!(
        journal
            .operations
            .values()
            .all(|entry| matches!(entry.state, OperationState::RetryRequired))
    );
    assert!(
        journal
            .operations
            .values()
            .all(|entry| entry.resources.len() >= 3)
    );
}

#[test]
fn concurrent_installers_do_not_replace_each_other() {
    let fixture: Fixture = Fixture::new();
    fixture.flag("verify-pause");
    let first: Child = fixture
        .command(&["install", "s", "--npx", "fixture"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let second: Child = fixture
        .command(&["install", "s", "--uvx", "fixture", "--skip-verify"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.flag("release");
    assert!(first.wait_with_output().unwrap().status.success());
    assert_eq!(second.wait_with_output().unwrap().status.code(), Some(3));
    assert!(!fixture.log().contains("uvx"));
}

#[test]
fn ownership_lock_waits_obey_deadline_without_running_runtime() {
    use std::os::unix::fs::PermissionsExt;
    let fixture: Fixture = Fixture::new();
    let directory: PathBuf = fixture
        .paths
        .directory(ripmcp::storage::Location::State)
        .unwrap();
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let lock: fs::File = fs::File::create(directory.join(".ownership.json.lock")).unwrap();
    lock.set_permissions(fs::Permissions::from_mode(0o600))
        .unwrap();
    lock.lock().unwrap();
    let output: Output = fixture.run(&["install", "s", "--npx", "fixture", "--timeout", "1"]);
    assert_eq!(output.status.code(), Some(9));
    assert!(!fixture.config().exists());
    assert!(fixture.log().is_empty());
}

#[test]
fn project_changes_during_verification_do_not_commit_or_renew_trust() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.root.path().join("project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        json!({"schema_version":1,"servers":{}}).to_string(),
    )
    .unwrap();
    let project: Project = Project::discover(&root).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    fixture.flag("verify-pause");
    let child: Child = fixture
        .command(&["install", "s", "--npx", "fixture", "--project"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let replacement = "{\"schema_version\":1,\"servers\":{}}\n";
    fs::write(root.join(".ripmcp/config.json"), replacement).unwrap();
    fixture.flag("release");
    let output: Output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(root.join(".ripmcp/config.json")).unwrap(),
        replacement
    );
    assert!(!fixture.config().exists());
}

#[test]
fn replacing_target_directory_during_verification_cannot_commit_elsewhere() {
    let fixture: Fixture = Fixture::new();
    fixture.flag("verify-pause");
    let child: Child = fixture
        .command(&["install", "s", "--npx", "fixture"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let directory: PathBuf = fixture.config().parent().unwrap().to_owned();
    fs::rename(&directory, directory.with_extension("moved")).unwrap();
    fs::create_dir(&directory).unwrap();
    fixture.flag("release");
    let output: Output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(!fixture.config().exists());
    assert!(
        !directory
            .with_extension("moved")
            .join("config.json")
            .exists()
    );
}

#[test]
fn importing_from_another_project_requires_and_rechecks_its_trust() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.root.path().join("source-project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        json!({"schema_version":1,"servers":{}}).to_string(),
    )
    .unwrap();
    let input: PathBuf = root.join("server.json");
    fs::write(&input, json!({"definition":{"kind":"local","runtime":"npx","package":"fixture","transport":"stdio"}}).to_string()).unwrap();
    let output: Output = fixture.run(&["install", "s", "--config", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(3));
    assert!(fixture.log().is_empty());
    let project: Project = Project::discover(&root).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    fixture.flag("verify-pause");
    let child: Child = fixture
        .command(&["install", "s", "--config", input.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    fs::write(root.join(".ripmcp/config.json"), "{\"schema_version\":1}\n").unwrap();
    fixture.flag("release");
    assert_eq!(child.wait_with_output().unwrap().status.code(), Some(3));
    assert!(!fixture.config().exists());
}

#[test]
fn trust_is_rechecked_between_resolution_and_package_execution() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.root.path().join("project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        json!({"schema_version":1,"servers":{}}).to_string(),
    )
    .unwrap();
    let project: Project = Project::discover(&root).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    fixture.flag("prep-pause");
    let child: Child = fixture
        .command(&["install", "s", "--npx", "fixture", "--project"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.wait("preparing");
    fs::write(root.join(".ripmcp/config.json"), "{\"schema_version\":1}\n").unwrap();
    fixture.flag("release");
    assert_eq!(child.wait_with_output().unwrap().status.code(), Some(3));
    assert!(!fixture.log().contains("npx"));
}
