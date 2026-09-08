use super::fixture::{Fixture, write};
use ripmcp::config::{Project, Source, schema::Runtime};
use ripmcp::trust::TrustStore;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Output};

#[test]
fn separate_calls_and_concurrent_first_calls_reuse_one_process() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    let mut children: Vec<Child> = (0..8)
        .map(|_| {
            fixture
                .command(fixture.root.path(), &["call", "s", "echo", "{}"])
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let first: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    let second: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(
        first["structuredContent"]["pid"],
        second["structuredContent"]["pid"]
    );
    assert_eq!(fixture.launches(), 1);
    assert_eq!(fixture.ok(&["start", "s"])["actions"][0]["reused"], true);
    fixture.ok(&["stop", "s"]);
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "stopped"
    );
}

#[test]
fn installation_bookkeeping_keeps_the_process_but_a_new_installation_does_not() {
    use ripmcp::ownership::{
        CleanupState, Origin, Ownership, OwnershipStore, Registration, Resource, ResourceIdentity,
        ResourceKind,
    };
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    fixture.ok(&["start", "s"]);
    let pid = fixture.pid();
    OwnershipStore::new(&fixture.paths)
        .unwrap()
        .update(|journal| {
            let entry: &mut ripmcp::ownership::Installation =
                journal.installations.values_mut().next().unwrap();
            entry.resources.push(Resource {
                kind: ResourceKind::Log,
                identity: ResourceIdentity::Path {
                    canonical_path: fixture.root.path().join("server/future.log"),
                },
                origin: Origin::Unknown,
                ownership: Ownership::Exclusive,
                cleanup: CleanupState::Pending,
            });
            Ok(())
        })
        .unwrap();
    assert_eq!(
        fixture.ok(&["call", "s", "echo", "{}"])["structuredContent"]["pid"],
        pid
    );
    OwnershipStore::new(&fixture.paths)
        .unwrap()
        .update(|journal| {
            journal
                .installations
                .values_mut()
                .next()
                .unwrap()
                .registration = Registration::Unregistered;
            Ok(())
        })
        .unwrap();
    fixture.install(
        Source::User {
            config: fixture.config(),
        },
        Runtime::Npx,
        "fixture@1.2.3",
    );
    fixture.ok(&["start", "s"]);
    assert_ne!(fixture.pid(), pid);
    assert_eq!(fixture.launches(), 2);
}

#[test]
fn disabled_servers_remain_running_but_cannot_be_used() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    fixture.ok(&["start", "s"]);
    let pid = fixture.pid();
    config["servers"]["s"]["enabled"] = json!(false);
    fixture.write_user(&config);
    for args in [
        &["start", "s"][..],
        &["tools", "s", "--all"],
        &["call", "s", "echo", "{}"],
    ] {
        assert_eq!(fixture.run(args).status.code(), Some(3));
    }
    let report: Value = fixture.ok(&["servers"]);
    assert_eq!(report["servers"][0]["enabled"], false);
    assert_eq!(report["servers"][0]["process_state"], "running");
    assert_eq!(report["servers"][0]["health"], "stale");
    assert_eq!(fixture.pid(), pid);
    fixture.ok(&["stop", "s"]);
}

#[test]
fn uvx_launches_the_recorded_exact_requirement() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    config["servers"]["s"]["definition"]["runtime"] = json!("uvx");
    fixture.write_user(&config);
    ripmcp::ownership::OwnershipStore::new(&fixture.paths)
        .unwrap()
        .update(|journal| {
            let entry: &mut ripmcp::ownership::Installation =
                journal.installations.values_mut().next().unwrap();
            let ripmcp::ownership::Origin::Local {
                runtime, resolved, ..
            }: &mut ripmcp::ownership::Origin = &mut entry.origin
            else {
                panic!("expected local installation");
            };
            *runtime = Runtime::Uvx;
            *resolved = Some("fixture==1.2.3".to_owned());
            Ok(())
        })
        .unwrap();
    fixture.ok(&["call", "s", "echo", "{}"]);
    let args: Value =
        ripmcp::json::parse(&fs::read(fixture.root.path().join("server/args")).unwrap()).unwrap();
    assert_eq!(args[0], "fixture==1.2.3");
}

#[test]
fn definition_and_environment_revisions_do_not_share_credentials() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    config["servers"]["s"]["definition"]["env"] = json!({"SECRET": {"env": "TEST_SECRET"}});
    fixture.write_user(&config);
    let mut pids: Vec<u32> = Vec::new();
    for secret in ["one", "two"] {
        let output: Output = fixture
            .command(fixture.root.path(), &["call", "s", "echo", "{}"])
            .env("TEST_SECRET", secret)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
        assert_eq!(value["structuredContent"]["secret"], secret);
        pids.push(fixture.pid());
    }
    assert_ne!(pids[0], pids[1]);
    config["servers"]["s"]["definition"]["args"][0] = json!("changed");
    config["servers"]["s"]["definition"]["env"] = json!({});
    fixture.write_user(&config);
    let value: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(value["structuredContent"]["mode"], "changed");
    assert_eq!(value["structuredContent"]["secret"], Value::Null);
    assert_eq!(fixture.launches(), 3);
}

#[test]
fn scoped_stop_never_stops_the_same_named_user_server() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("user");
    fixture.ok(&["start", "s"]);
    let user_pid = fixture.pid();
    let project: PathBuf = fixture.root.path().join("project");
    let data: PathBuf = project.join("server");
    config["servers"]["s"]["definition"]["args"] = json!(["project", data]);
    write(&project.join(".ripmcp/config.json"), &config);
    assert_eq!(
        fixture
            .command(&project, &["start", "s"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(3)
    );
    let selected: Project = Project::discover(&project).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&selected)
        .unwrap();
    fixture.install(
        Source::Project {
            root: project.canonicalize().unwrap(),
        },
        Runtime::Npx,
        "fixture@1.2.3",
    );
    assert!(
        fixture
            .command(&project, &["start", "s"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        fixture
            .command(&project, &["stop", "s"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let value: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(value["structuredContent"]["pid"], user_pid);
}

#[test]
fn passive_status_and_remote_lifecycle_never_launch_anything() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    config["servers"]["remote"] = json!({"definition": {"kind": "remote", "url": "https://example.invalid/mcp", "transport": "streamable_http"}});
    fixture.write_user(&config);
    let report: Value = fixture.ok(&["servers"]);
    assert_eq!(report["servers"][0]["process_state"], "not_managed");
    assert_eq!(report["servers"][0]["health"], "unknown");
    assert_eq!(report["servers"][1]["health"], "unverified");
    for verb in ["start", "stop"] {
        assert_eq!(fixture.run(&[verb, "remote"]).status.code(), Some(10));
    }
    assert!(!fixture.runtime().exists());
    assert!(!fixture.root.path().join("server/pid").exists());
}

#[test]
fn interrupted_calls_are_not_replayed_and_next_use_restarts() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("crash_call");
    assert_eq!(
        fixture.run(&["call", "s", "echo", "{}"]).status.code(),
        Some(4)
    );
    assert_eq!(fixture.launches(), 1);
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    fixture.ok(&["start", "s"]);
    assert_eq!(fixture.launches(), 2);
}
