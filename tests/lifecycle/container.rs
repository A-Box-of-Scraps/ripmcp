use super::fixture::Fixture;
use ripmcp::config::schema::Runtime;
use ripmcp::ownership::{Origin, OwnershipStore};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

fn prepare(fixture: &Fixture) -> PathBuf {
    let mut config: Value = fixture.configure("echo");
    let root: PathBuf = fixture.root.path().join("engine");
    fs::create_dir(&root).unwrap();
    config["servers"]["s"]["definition"]["runtime"] = json!("docker");
    config["servers"]["s"]["definition"]["env"] = json!({"DOCKER_ROOT": {"env": "TEST_ENGINE"}});
    fixture.write_user(&config);
    OwnershipStore::new(&fixture.paths)
        .unwrap()
        .update(|journal| {
            for entry in journal.installations.values_mut() {
                if let Origin::Local {
                    runtime, resolved, ..
                } = &mut entry.origin
                {
                    *runtime = Runtime::Docker;
                    *resolved = Some(format!("fixture@sha256:{}", "a".repeat(64)));
                }
            }
            Ok(())
        })
        .unwrap();
    root
}

#[test]
fn docker_stop_removes_only_the_labelled_owned_container() {
    let fixture: Fixture = Fixture::new();
    let engine: PathBuf = prepare(&fixture);
    let foreign: PathBuf = engine.join(format!("{}.json", "b".repeat(64)));
    fs::write(
        &foreign,
        r#"{"name":"foreign","label":"io.ripmcp.owner=foreign"}"#,
    )
    .unwrap();
    let output: std::process::Output = fixture
        .command(fixture.root.path(), &["start", "s"])
        .env("TEST_ENGINE", &engine)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_dir(&engine).unwrap().count(), 2);
    fixture.ok(&["stop", "s"]);
    assert!(foreign.exists());
    assert_eq!(fs::read_dir(&engine).unwrap().count(), 1);
}

#[test]
fn docker_cleanup_failure_is_explicit_and_prevents_double_launch() {
    let fixture: Fixture = Fixture::new();
    let engine: PathBuf = prepare(&fixture);
    assert!(
        fixture
            .command(fixture.root.path(), &["start", "s"])
            .env("TEST_ENGINE", &engine)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(engine.join("fail"), "").unwrap();
    assert_eq!(fixture.run(&["stop", "s"]).status.code(), Some(8));
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "cleanup_required"
    );
    assert!(
        !fixture
            .command(fixture.root.path(), &["start", "s"])
            .env("TEST_ENGINE", &engine)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fixture.launches(), 1);
}
