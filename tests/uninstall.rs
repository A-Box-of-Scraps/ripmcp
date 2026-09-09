#[allow(dead_code)]
#[path = "install/fixture.rs"]
mod fixture;
#[path = "uninstall/recovery.rs"]
mod recovery;
#[path = "uninstall/terminal.rs"]
mod terminal;

use fixture::Fixture;
use ripmcp::ownership::{
    CleanupState, Filesystem, Journal, Origin, Ownership, OwnershipStore, Registration, Resource,
    ResourceIdentity, ResourceKind,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

fn track(fixture: &Fixture, path: &Path) {
    OwnershipStore::new(&fixture.paths)
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
                        canonical_path: path.to_path_buf(),
                    },
                    origin: Origin::Standalone,
                    ownership: Ownership::Exclusive,
                    cleanup: CleanupState::Pending,
                    filesystem: Some(Filesystem::created(path, fixture.root.path()).unwrap()),
                });
            Ok(())
        })
        .unwrap();
}

fn install(fixture: &Fixture) {
    let _: Value = fixture.ok(&["install", "s", "--npx", "fixture"]);
}

#[test]
fn ordinary_uninstall_stops_only_selected_process_and_retains_data() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let _: Value = fixture.ok(&["install", "other", "--npx", "fixture"]);
    let path: PathBuf = fixture.root.path().join("owned-data");
    fs::write(&path, "retained").unwrap();
    track(&fixture, &path);
    let report: Value = fixture.ok(&["uninstall", "s"]);
    assert_eq!(report["failures"], json!([]));
    assert!(path.exists());
    let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
    assert_eq!(
        journal
            .installations
            .values()
            .find(|entry| entry.server == "s")
            .unwrap()
            .registration,
        Registration::Unregistered
    );
    let report: Value = fixture.ok(&["servers"]);
    assert_eq!(report["servers"][0]["name"], "other");
    assert_eq!(report["servers"][0]["process_state"], "running");
    assert!(!fixture.run(&["start", "s"]).status.success());
}

#[test]
fn clean_requires_confirmation_before_any_mutation() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let before: Vec<u8> = fs::read(fixture.config()).unwrap();
    let journal: PathBuf = fixture
        .paths
        .directory(ripmcp::storage::Location::State)
        .unwrap()
        .join("ownership.json");
    let ownership: Vec<u8> = fs::read(&journal).unwrap();
    let output: Output = fixture.run(&["uninstall", "s", "--clean"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(fs::read(fixture.config()).unwrap(), before);
    assert_eq!(fs::read(journal).unwrap(), ownership);
    assert_eq!(fixture.ok(&["start", "s"])["actions"][0]["reused"], true);
}

#[test]
fn clean_removes_only_recorded_files_and_empty_directories_and_retries() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let directory: PathBuf = fixture.root.path().join("custom-installation");
    fs::create_dir(&directory).unwrap();
    let path: PathBuf = directory.join("owned");
    fs::write(&path, "owned").unwrap();
    track(&fixture, &directory);
    track(&fixture, &path);
    fs::write(directory.join("untracked"), "preserve").unwrap();
    let output: Output = fixture.run(&["uninstall", "s", "--clean", "-y"]);
    assert_eq!(output.status.code(), Some(8), "{output:?}");
    assert!(!path.exists());
    assert!(directory.join("untracked").exists());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["failures"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["plan"]["retry"],
        json!(["ripmcp", "uninstall", "s", "--user", "--clean", "-y"])
    );
    fs::remove_file(directory.join("untracked")).unwrap();
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(!directory.exists());
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
}

#[test]
fn substituted_file_and_symlink_parent_are_never_followed() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let directory: PathBuf = fixture.root.path().join("owned");
    fs::create_dir(&directory).unwrap();
    let path: PathBuf = directory.join("file");
    fs::write(&path, "owned").unwrap();
    track(&fixture, &path);
    fs::rename(&path, directory.join("original")).unwrap();
    fs::write(&path, "foreign").unwrap();
    assert_eq!(
        fixture
            .run(&["uninstall", "s", "--clean", "-y"])
            .status
            .code(),
        Some(8)
    );
    assert_eq!(fs::read(&path).unwrap(), b"foreign");
    fs::rename(&directory, fixture.root.path().join("saved")).unwrap();
    std::os::unix::fs::symlink(fixture.root.path().join("saved"), &directory).unwrap();
    assert_eq!(
        fixture
            .run(&["uninstall", "s", "--clean", "-y"])
            .status
            .code(),
        Some(8)
    );
    assert_eq!(fs::read(&path).unwrap(), b"foreign");
}

#[test]
fn duplicate_ownership_and_project_paths_are_preserved() {
    let fixture: Fixture = Fixture::new();
    install(&fixture);
    let path: PathBuf = fixture.root.path().join("duplicate");
    fs::write(&path, "keep").unwrap();
    track(&fixture, &path);
    track(&fixture, &path);
    let project: PathBuf = fixture.root.path().join("project/.ripmcp");
    fs::create_dir_all(&project).unwrap();
    let config: PathBuf = project.join("config.json");
    fs::write(&config, "keep").unwrap();
    track(&fixture, &config);
    let report: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(report["plan"]["deletable"].as_array().unwrap().is_empty());
    assert!(path.exists());
    assert!(config.exists());
}

#[test]
fn remote_uninstall_only_unregisters_local_state() {
    let fixture: Fixture = Fixture::new();
    let path: PathBuf = fixture.import(&json!({"definition": {"kind": "remote", "url": "http://127.0.0.1:1/mcp", "transport": "streamable_http", "authentication": "none"}}));
    let _: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(fixture.log().is_empty());
    assert!(path.exists());
}
