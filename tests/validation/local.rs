use super::fixture::Fixture;
use ripmcp::ownership::{
    CleanupState, Filesystem, Journal, Origin, Ownership, OwnershipStore, Registration, Resource,
    ResourceIdentity, ResourceKind,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Output};

fn owned_data(fixture: &Fixture, name: &str) -> PathBuf {
    let path: PathBuf = fixture.root.path().join(name);
    fs::write(&path, "owned fixture data").unwrap();
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
                        canonical_path: path.clone(),
                    },
                    origin: Origin::Standalone,
                    ownership: Ownership::Exclusive,
                    cleanup: CleanupState::Pending,
                    filesystem: Some(Filesystem::created(&path, fixture.root.path()).unwrap()),
                });
            Ok(())
        })
        .unwrap();
    path
}

fn launches(fixture: &Fixture) -> usize {
    fs::read_to_string(fixture.root.path().join("launches"))
        .unwrap()
        .lines()
        .count()
}

fn exercise_policy(fixture: &Fixture) {
    let _: Value = fixture.ok(&["disable", "s", "echo", "--user"]);
    assert_eq!(fixture.ok(&["tools", "s"])["tools"], json!([]));
    assert_eq!(
        fixture.ok(&["tools", "s", "--all"])["tools"][0]["enabled"],
        false
    );
    for (args, code) in [
        (&["call", "s", "echo", "{}"][..], 3),
        (&["call", "echo", "{}"][..], 2),
    ] {
        assert_eq!(fixture.run(args).status.code(), Some(code));
    }
    assert!(!fixture.log().contains("tools/call"));
    let _: Value = fixture.ok(&["enable", "s", "echo"]);
    let result: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(result, json!({"resultType":"complete", "content":[]}));
    let count = launches(fixture);
    assert_eq!(fixture.ok(&["call", "echo", "{}"]), result);
    assert_eq!(launches(fixture), count);
    assert_eq!(
        fixture
            .log()
            .lines()
            .filter(|line| *line == "tools/call")
            .count(),
        2
    );
}

fn exercise_stop_and_disable(fixture: &Fixture) {
    let count = launches(fixture);
    let _: Value = fixture.ok(&["stop", "s"]);
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "stopped"
    );
    assert_eq!(launches(fixture), count);
    let _: Value = fixture.ok(&["call", "echo", "{}"]);
    assert_eq!(launches(fixture), count + 1);
    let _: Value = fixture.ok(&["disable", "s"]);
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "running"
    );
    let _: Value = fixture.ok(&["stop", "s"]);
    let before: String = fixture.log();
    for args in [
        &["start", "s"][..],
        &["tools", "s"],
        &["call", "s", "echo", "{}"],
    ] {
        assert_eq!(fixture.run(args).status.code(), Some(3));
    }
    assert_eq!(fixture.log(), before);
    let _: Value = fixture.ok(&["enable", "s", "--user"]);
}

#[test]
fn installed_npx_lifecycle_across_processes() {
    exercise_local_lifecycle("--npx");
}

#[test]
fn installed_uvx_lifecycle_across_processes() {
    exercise_local_lifecycle("--uvx");
}

#[test]
fn installed_docker_lifecycle_across_processes() {
    exercise_local_lifecycle("--docker");
}

fn exercise_local_lifecycle(runtime: &str) {
    let fixture: Fixture = Fixture::new();
    let installed: Value = fixture.ok(&["install", "s", runtime, "fixture", "--user"]);
    assert_eq!(installed["installation"]["verification"], "verified");
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "running"
    );
    assert_eq!(fixture.ok(&["tool", "s", "echo"])["tool"]["name"], "echo");
    assert!(!fixture.log().contains("tools/call"));
    exercise_policy(&fixture);
    exercise_stop_and_disable(&fixture);
    exercise_reinstall_and_cleanup(&fixture, runtime);
    assert_eq!(
        fixture
            .log()
            .lines()
            .filter(|line| *line == "tools/call")
            .count(),
        3
    );
}

fn exercise_reinstall_and_cleanup(fixture: &Fixture, runtime: &str) {
    let retained: PathBuf = owned_data(fixture, "retained");
    let _: Value = fixture.ok(&["uninstall", "s"]);
    assert!(retained.exists());
    assert_eq!(fixture.ok(&["servers"])["servers"], json!([]));
    let journal: Journal = OwnershipStore::new(&fixture.paths).unwrap().read().unwrap();
    assert_eq!(journal.installations.len(), 1);
    assert_eq!(
        journal.installations.values().next().unwrap().registration,
        Registration::Unregistered
    );
    let count = launches(fixture);
    let _: Value = fixture.ok(&["install", "s", runtime, "fixture"]);
    assert_eq!(launches(fixture), count + 1);
    let cleaned: PathBuf = owned_data(fixture, "cleaned");
    let declined: Output = fixture.run(&["uninstall", "s", "--clean"]);
    assert_eq!(declined.status.code(), Some(3));
    assert_eq!(fixture.ok(&["start", "s"])["actions"][0]["reused"], true);
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert!(!cleaned.exists());
    assert!(retained.exists());
    assert_eq!(fixture.ok(&["servers"])["servers"], json!([]));
}
