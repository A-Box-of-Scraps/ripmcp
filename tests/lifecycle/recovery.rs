use super::fixture::{Fixture, write};
use ripmcp::config::Project;
use ripmcp::ownership::{Origin, OwnershipStore};
use ripmcp::trust::TrustStore;
use rustix::fd::OwnedFd;
use rustix::process::{Pid, PidfdFlags, Signal};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

fn signal(pid: u32, signal: Signal) {
    let pid: Pid = Pid::from_raw(pid as i32).unwrap();
    let fd: OwnedFd = rustix::process::pidfd_open(pid, PidfdFlags::empty()).unwrap();
    rustix::process::pidfd_send_signal(fd, signal).unwrap();
}

fn wait_file(path: &Path) {
    let until: Instant = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < until, "fixture did not become ready");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_dead(pid: u32) {
    let until: Instant = Instant::now() + Duration::from_secs(5);
    while let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
        if stat
            .rsplit_once(") ")
            .is_some_and(|(_, fields)| fields.starts_with('Z'))
        {
            return;
        }
        assert!(Instant::now() < until, "owned process survived teardown");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn stopping_an_owned_group_reaps_grandchildren_and_preserves_foreign_processes() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("grandchild");
    fixture.ok(&["start", "s"]);
    let pid = fixture.pid();
    let grandchild: u32 = fs::read_to_string(fixture.root.path().join("server/grandchild"))
        .unwrap()
        .parse()
        .unwrap();
    let mut foreign: Child = std::process::Command::new("/usr/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    fixture.ok(&["stop", "s"]);
    wait_dead(pid);
    wait_dead(grandchild);
    assert!(foreign.try_wait().unwrap().is_none());
    foreign.kill().unwrap();
    foreign.wait().unwrap();
}

#[test]
fn supervisor_sigkill_tears_down_owned_groups_and_next_use_does_not_double_launch() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("grandchild");
    fixture.ok(&["start", "s"]);
    let old = fixture.pid();
    let grandchild: u32 = fs::read_to_string(fixture.root.path().join("server/grandchild"))
        .unwrap()
        .parse()
        .unwrap();
    let record: Value =
        ripmcp::json::parse(&fs::read(fixture.runtime().join("supervisor.json")).unwrap()).unwrap();
    signal(record["pid"].as_u64().unwrap() as u32, Signal::KILL);
    wait_dead(record["pid"].as_u64().unwrap() as u32);
    let passive: Value = fixture.ok(&["servers"]);
    assert!(matches!(
        passive["servers"][0]["process_state"].as_str(),
        Some("stopped" | "unknown")
    ));
    assert_eq!(
        ripmcp::json::parse(&fs::read(fixture.runtime().join("supervisor.json")).unwrap()).unwrap(),
        record
    );
    fixture.ok(&["start", "s"]);
    assert_ne!(old, fixture.pid());
    assert_eq!(fixture.launches(), 2);
    wait_dead(old);
    wait_dead(grandchild);
}

#[test]
fn cancelled_start_cleans_up_only_its_new_child_and_next_start_recovers() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("stall_init");
    assert_eq!(
        fixture.run(&["--timeout", "1", "start", "s"]).status.code(),
        Some(9)
    );
    wait_dead(fixture.pid());
    config["servers"]["s"]["definition"]["args"][0] = json!("echo");
    fixture.write_user(&config);
    fixture.ok(&["start", "s"]);
    assert_eq!(fixture.launches(), 2);
}

#[test]
fn disconnected_calls_are_cancelled_without_killing_or_replaying_the_server() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("cancel_ack");
    let mut cli: Child = fixture
        .command(
            fixture.root.path(),
            &["call", "s", "echo", "{\"stall\":true}"],
        )
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    wait_file(&fixture.root.path().join("server/calls"));
    let pid = fixture.pid();
    cli.kill().unwrap();
    cli.wait().unwrap();
    wait_file(&fixture.root.path().join("server/cancelled"));
    let value: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(value["structuredContent"]["pid"], pid);
    assert_eq!(fixture.launches(), 1);
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn interrupting_the_cli_process_group_does_not_kill_the_reused_server() {
    use std::os::unix::process::CommandExt;
    let fixture: Fixture = Fixture::new();
    fixture.configure("cancel_ack");
    let mut cli: Child = fixture
        .command(
            fixture.root.path(),
            &["call", "s", "echo", "{\"stall\":true}"],
        )
        .process_group(0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_file(&fixture.root.path().join("server/calls"));
    let pid = fixture.pid();
    rustix::process::kill_process_group(Pid::from_raw(cli.id() as i32).unwrap(), Signal::INT)
        .unwrap();
    assert_eq!(cli.wait().unwrap().code(), Some(130));
    let value: Value = fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(value["structuredContent"]["pid"], pid);
    assert_eq!(fixture.launches(), 1);
}

#[test]
fn ignored_cancellation_does_not_accumulate_unbounded_orphan_calls() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    let mut calls = 0;
    for abandoned in 1..=16 {
        let mut cli: Child = fixture
            .command(
                fixture.root.path(),
                &["call", "s", "echo", "{\"stall\":true}"],
            )
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        calls += 1;
        wait_lines(&fixture.root.path().join("server/calls"), calls);
        cli.kill().unwrap();
        cli.wait().unwrap();
        wait_lines(&fixture.root.path().join("server/cancelled"), abandoned);
        if abandoned == 1 {
            let pid = fixture.pid();
            assert_eq!(
                fixture.ok(&["call", "s", "echo", "{}"])["structuredContent"]["pid"],
                pid
            );
            calls += 1;
        }
    }
    assert_eq!(
        fixture
            .run(&["--timeout", "1", "call", "s", "echo", "{}"])
            .status
            .code(),
        Some(9)
    );
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        17
    );
    assert_eq!(fixture.ok(&["servers"])["servers"][0]["health"], "unknown");
    fixture.ok(&["stop", "s"]);
    fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(fixture.launches(), 2);
}

fn wait_lines(path: &Path, count: usize) {
    let until: Instant = Instant::now() + Duration::from_secs(5);
    while fs::read_to_string(path).unwrap_or_default().lines().count() < count {
        assert!(Instant::now() < until, "fixture request did not arrive");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn changed_project_trust_is_enforced_even_by_an_existing_supervisor() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    let project: std::path::PathBuf = fixture.root.path().join("project");
    write(&project.join(".ripmcp/config.json"), &config);
    let selected: Project = Project::discover(&project).unwrap().unwrap();
    TrustStore::new(&fixture.paths)
        .unwrap()
        .approve(&selected)
        .unwrap();
    fixture.install(
        ripmcp::config::Source::Project {
            root: project.clone(),
        },
        ripmcp::config::schema::Runtime::Npx,
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
    config["servers"]["s"]["definition"]["args"][0] = json!("changed");
    write(&project.join(".ripmcp/config.json"), &config);
    assert_eq!(
        fixture
            .command(&project, &["call", "s", "echo", "{}"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(3)
    );
    assert_eq!(fixture.launches(), 1);
    assert!(
        fixture
            .command(&project, &["stop", "s"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn missing_or_floating_installation_resolution_never_launches_a_runtime() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    for resolution in [None, Some("fixture@latest"), Some("fixture@1moving-tag")] {
        OwnershipStore::new(&fixture.paths)
            .unwrap()
            .update(|journal| {
                let entry: &mut ripmcp::ownership::Installation =
                    journal.installations.values_mut().next().unwrap();
                let Origin::Local { resolved, .. }: &mut Origin = &mut entry.origin else {
                    panic!("expected local installation");
                };
                *resolved = resolution.map(str::to_owned);
                Ok(())
            })
            .unwrap();
        assert_eq!(fixture.run(&["start", "s"]).status.code(), Some(3));
    }
    assert!(!fixture.root.path().join("server/pid").exists());
}

#[test]
fn policy_is_rechecked_after_waiting_for_a_surviving_owners_lease() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::OpenOptionsExt;
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    let source: ripmcp::config::Source = ripmcp::config::Source::User {
        config: fixture.config().canonicalize().unwrap(),
    };
    let bytes: Vec<u8> = serde_json::to_vec(&(source, "s")).unwrap();
    let key: String = format!("{:x}", Sha256::digest(bytes));
    let lock: fs::File = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(
            fixture
                .paths
                .directory(ripmcp::storage::Location::State)
                .unwrap()
                .join(format!(".instance-{key}.lock")),
        )
        .unwrap();
    lock.lock().unwrap();
    let mut cli: Child = fixture
        .command(fixture.root.path(), &["start", "s"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let until: Instant = Instant::now() + Duration::from_secs(5);
    while fixture.ok(&["servers"])["servers"][0]["process_state"] != "busy" {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    config["servers"]["s"]["enabled"] = json!(false);
    fixture.write_user(&config);
    drop(lock);
    assert_eq!(cli.wait().unwrap().code(), Some(3));
    assert!(!fixture.root.path().join("server/pid").exists());
}
