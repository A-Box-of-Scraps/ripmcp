use super::*;
use ripmcp::{
    config::{Project, Source},
    trust::TrustStore,
};
use std::{
    os::unix::fs::PermissionsExt,
    process::{Child, Stdio},
    time::Duration,
};

#[test]
fn permission_failure_retains_retry_state_and_reports_cause() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let directory: PathBuf = fixture.root.path().join("readonly");
    fs::create_dir(&directory).unwrap();
    let path: PathBuf = directory.join("data");
    fs::write(&path, "keep until writable").unwrap();
    track(&fixture, &path);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    let output: Output = fixture.run(&["uninstall", "s", "--clean", "-y"]);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(path.exists());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["failures"][0]["cause"]
            .as_str()
            .unwrap()
            .contains("Permission denied")
    );
    let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
    assert!(journal.operations.values().any(|entry| matches!(
        entry.state,
        ripmcp::ownership::OperationState::RetryRequired
    )));
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(!path.exists());
}

#[test]
fn interruption_after_quarantine_rename_is_recoverable() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let path: PathBuf = fixture.root.path().join("data");
    fs::write(&path, "owned").unwrap();
    track(&fixture, &path);
    let proof: Filesystem = Filesystem::created(&path, fixture.root.path()).unwrap();
    let staged: PathBuf = fixture
        .root
        .path()
        .join(format!(".ripmcp-remove-{}-{}", proof.device, proof.inode));
    fs::rename(&path, &staged).unwrap();
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(!path.exists());
    assert!(!staged.exists());
}

#[test]
fn ambiguous_orphan_records_fail_without_mutation() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let _: Value = fixture.ok(&["uninstall", "s"]);
    install(&fixture);
    let _: Value = fixture.ok(&["uninstall", "s"]);
    let bytes: Vec<u8> = fs::read(fixture.config()).unwrap();
    let output: Output = fixture.run(&["uninstall", "s", "--clean", "-y"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("ambiguous"));
    assert_eq!(fs::read(fixture.config()).unwrap(), bytes);
}

#[test]
fn pending_clean_retry_resolves_its_record_after_an_older_ordinary_uninstall() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let _: Value = fixture.ok(&["uninstall", "s"]);
    install(&fixture);
    let data: PathBuf = fixture.root.path().join("latest-data");
    fs::write(&data, "owned").unwrap();
    OwnershipStore::new(&fixture.paths)
        .unwrap()
        .update(|journal| {
            journal
                .installations
                .values_mut()
                .find(|entry| entry.registration == Registration::Registered)
                .unwrap()
                .resources
                .push(Resource {
                    kind: ResourceKind::Data,
                    identity: ResourceIdentity::Path {
                        canonical_path: data.clone(),
                    },
                    origin: Origin::Standalone,
                    ownership: Ownership::Exclusive,
                    cleanup: CleanupState::Pending,
                    filesystem: Some(Filesystem::created(&data, fixture.root.path()).unwrap()),
                });
            Ok(())
        })
        .unwrap();
    fs::rename(&data, data.with_extension("original")).unwrap();
    fs::write(&data, "replacement").unwrap();
    assert_eq!(
        fixture
            .run(&["uninstall", "s", "--clean", "-y"])
            .status
            .code(),
        Some(8)
    );
    fs::remove_file(&data).unwrap();
    fs::rename(data.with_extension("original"), &data).unwrap();
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(!data.exists());
}

#[test]
fn maintenance_waits_for_active_calls_and_blocks_concurrent_auto_start() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    fixture.flag("verify-pause");
    let mut call: Child = fixture
        .command(&["tools", "s"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    fixture.wait("verifying");
    let before: Vec<u8> = fs::read(fixture.config()).unwrap();
    let output: Output = fixture.run(&["uninstall", "s", "--clean", "-y", "--timeout", "1"]);
    assert_eq!(output.status.code(), Some(9), "{output:?}");
    assert_eq!(fs::read(fixture.config()).unwrap(), before);
    let removal: Child = fixture
        .command(&["uninstall", "s", "--clean", "-y"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let start: Child = fixture
        .command(&["start", "s"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    fixture.flag("release");
    assert!(call.wait().unwrap().success());
    let output: Output = removal.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!start.wait_with_output().unwrap().status.success());
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn selected_scope_does_not_stop_or_unregister_a_project_override() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let root: PathBuf = fixture.root.path().join("project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        fs::read(fixture.config()).unwrap(),
    )
    .unwrap();
    let project: Project = Project::discover(&root).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&project)
        .unwrap();
    let bytes: Vec<u8> = fs::read(root.join(".ripmcp/config.json")).unwrap();
    let output: Output = fixture
        .command(&["uninstall", "s", "--clean", "-y"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["plan"]["shadowed_by_trusted_project"], true);
    assert_eq!(fs::read(root.join(".ripmcp/config.json")).unwrap(), bytes);
    let output: Output = fixture
        .command(&["uninstall", "s", "--project"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        !TrustStore::new(&fixture.paths)
            .unwrap()
            .is_approved(&Project::discover(&root).unwrap().unwrap())
            .unwrap()
    );
    let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
    assert!(
        journal
            .installations
            .values()
            .all(|entry| matches!(entry.scope, Source::User { .. }))
    );
}

#[test]
fn failed_owned_container_stop_preserves_configuration_and_data() {
    let fixture: Fixture = Fixture::new();
    let _: Value = fixture.ok(&["install", "s", "--docker", "fixture"]);
    let path: PathBuf = fixture.root.path().join("data");
    fs::write(&path, "preserve").unwrap();
    track(&fixture, &path);
    let configuration: Vec<u8> = fs::read(fixture.config()).unwrap();
    fixture.flag("cleanup-fail");
    let output: Output = fixture.run(&["uninstall", "s", "--clean", "-y"]);
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert_eq!(fs::read(fixture.config()).unwrap(), configuration);
    assert!(path.exists());
}

#[test]
fn project_clean_retry_after_unregistration_does_not_need_execution_trust() {
    let fixture: Fixture = Fixture::new();
    let root: PathBuf = fixture.root.path().join("project");
    fs::create_dir_all(root.join(".ripmcp")).unwrap();
    fs::write(
        root.join(".ripmcp/config.json"),
        r#"{"schema_version":1,"servers":{}}"#,
    )
    .unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&Project::discover(&root).unwrap().unwrap())
        .unwrap();
    let output: Output = fixture
        .command(&["install", "s", "--npx", "fixture", "--project"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&Project::discover(&root).unwrap().unwrap())
        .unwrap();
    let data: PathBuf = root.join("owned-data");
    fs::write(&data, "owned").unwrap();
    track(&fixture, &data);
    fs::rename(&data, root.join("original")).unwrap();
    fs::write(&data, "replacement").unwrap();
    let args: [&str; 5] = ["uninstall", "s", "--project", "--clean", "-y"];
    let output: Output = fixture.command(&args).current_dir(&root).output().unwrap();
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(
        !TrustStore::new(&fixture.paths)
            .unwrap()
            .is_approved(&Project::discover(&root).unwrap().unwrap())
            .unwrap()
    );
    fs::remove_file(&data).unwrap();
    fs::rename(root.join("original"), &data).unwrap();
    let output: Output = fixture.command(&args).current_dir(&root).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!data.exists());
    assert!(root.join(".ripmcp/config.json").exists());
}
