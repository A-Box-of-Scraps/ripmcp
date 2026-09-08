#[path = "lifecycle/fixture.rs"]
mod fixture;
use fixture::Fixture;
use serde_json::{Value, json};
use std::{fs, process::Output};

#[test]
fn policy_persists_and_lists_are_concise() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    let listing: Value = fixture.ok(&["tools"]);
    assert_eq!(
        listing,
        json!({"schema_version": 1, "tools": [{"server": "s", "name": "echo", "enabled": true}], "errors": []})
    );
    assert_eq!(
        fixture.ok(&["tool", "s", "echo"])["tool"]["inputSchema"],
        json!({"type": "object"})
    );
    fixture.ok(&["disable", "s", "echo"]);
    assert_eq!(fixture.ok(&["tools", "s"])["tools"], json!([]));
    assert_eq!(
        fixture.ok(&["tools", "s", "--all"])["tools"][0]["enabled"],
        false
    );
    assert_eq!(
        fixture.run(&["call", "s", "echo", "{}"]).status.code(),
        Some(3)
    );
    assert_eq!(fixture.run(&["call", "echo", "{}"]).status.code(), Some(2));
    fixture.ok(&["stop", "s"]);
    assert_eq!(
        fixture.run(&["call", "s", "echo", "{}"]).status.code(),
        Some(3)
    );
    fixture.ok(&["enable", "s", "echo"]);
    fixture.ok(&["call", "echo", "{}"]);
    assert_eq!(
        fixture.run(&["disable", "s", "missing"]).status.code(),
        Some(2)
    );
    assert_eq!(fixture.run(&["enable", "missing"]).status.code(), Some(3));
}

#[test]
fn server_disable_keeps_process_and_retained_tool_policy() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("normal");
    config["servers"]["s"]["disabled_tools"] = json!(["old_tool"]);
    fixture.write_user(&config);
    fixture.ok(&["call", "echo", "{}"]);
    let pid = fixture.pid();
    fixture.ok(&["disable", "s"]);
    assert_eq!(fixture.pid(), pid);
    assert_eq!(fixture.ok(&["tools"])["tools"], json!([]));
    for args in [
        &["start", "s"][..],
        &["tools", "s", "--all"],
        &["call", "s", "echo", "{}"],
    ] {
        assert_eq!(fixture.run(args).status.code(), Some(3));
    }
    fixture.ok(&["enable", "s", "old_tool"]);
    fixture.ok(&["enable", "s"]);
    assert_eq!(fixture.ok(&["tools"])["tools"][0]["enabled"], true);
}

#[test]
fn incomplete_discovery_refuses_shorthand_but_qualification_bypasses_outage() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("normal");
    config["servers"]["offline"] = json!({"definition": {"kind": "remote", "url": "http://127.0.0.1:1/mcp", "transport": "streamable_http"}});
    fixture.write_user(&config);
    for args in [&["tools"][..], &["call", "echo", "{}"]] {
        let output: Output = fixture.run(args);
        assert_eq!(output.status.code(), Some(8));
        let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
        assert_eq!(value["errors"][0]["server"], "offline");
        assert_eq!(value["tools"][0]["server"], "s");
    }
    assert!(!fixture.root.path().join("server/calls").exists());
    fixture.ok(&["call", "s", "echo", "{}"]);
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn collision_lists_qualified_candidates_and_dispatches_nothing() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("normal");
    config["servers"]["other"] = config["servers"]["s"].clone();
    fixture.write_user(&config);
    let store: ripmcp::ownership::OwnershipStore =
        ripmcp::ownership::OwnershipStore::new(&fixture.paths).unwrap();
    store
        .update(|journal| {
            let entry: &ripmcp::ownership::Installation =
                journal.installations.values().next().unwrap();
            let new: ripmcp::ownership::Installation = ripmcp::ownership::Installation::new(
                entry.scope.clone(),
                "other".to_owned(),
                entry.origin.clone(),
            )
            .unwrap();
            journal
                .installations
                .insert(new.id().as_str().to_owned(), new);
            Ok(())
        })
        .unwrap();
    let output: Output = fixture.run(&["call", "echo", "{}"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        ripmcp::json::parse(&output.stdout).unwrap(),
        json!({"schema_version": 1, "candidates": [{"server": "other", "tool": "echo"}, {"server": "s", "tool": "echo"}]})
    );
    assert!(!fixture.root.path().join("server/calls").exists());
}

#[test]
fn call_once_save_shape_and_reuse_with_all_input_forms() {
    use std::io::Write;
    use std::process::{Child, Stdio};
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    let input: std::path::PathBuf = fixture.root.path().join("arguments.json");
    fs::write(&input, br#"{"value":"\u00e9","array":[null,42,true]}"#).unwrap();
    for qualified in [false, true] {
        for source in ["inline", "file", "stdin"] {
            let mut args: Vec<&str> = vec!["call"];
            if qualified {
                args.push("s");
            }
            args.push("echo");
            match source {
                "inline" => args.push(r#"{"value":"\u00e9","array":[null,42,true]}"#),
                "file" => args.extend(["--input", input.to_str().unwrap()]),
                _ => args.extend(["--input", "-"]),
            }
            let mut child: Child = fixture
                .command(fixture.root.path(), &args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&fs::read(&input).unwrap())
                .unwrap();
            let output: Output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
            assert_eq!(value["structuredContent"]["arguments"]["value"], "\u{e9}");
            let saved: std::path::PathBuf = fixture.root.path().join("result.json");
            fs::write(&saved, &output.stdout).unwrap();
            let shaped: Value = fixture.ok(&["shape", saved.to_str().unwrap()]);
            assert_eq!(
                shaped["shape"]["fields"]["structuredContent"]["fields"]["arguments"]["fields"]["array"]
                    ["length"],
                3
            );
        }
    }
    assert_eq!(fixture.launches(), 1);
    assert_eq!(
        fixture.pid() as u64,
        fixture.ok(&["call", "echo", "{}"])["structuredContent"]["pid"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        7
    );
}

#[test]
fn malformed_and_missing_inputs_never_dispatch_or_echo_secrets() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    let input: std::path::PathBuf = fixture.root.path().join("input.json");
    for bytes in [
        b"secret".as_slice(),
        b"[]",
        br#"{"secret":1,"secret":2}"#,
        b"\xff",
    ] {
        fs::write(&input, bytes).unwrap();
        let output: Output =
            fixture.run(&["call", "s", "echo", "--input", input.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret"));
    }
    assert!(!fixture.root.path().join("server/calls").exists());
    assert!(!fixture.runtime().join("supervisor.sock").exists());
}

#[test]
fn large_envelopes_and_tool_failures_are_not_truncated_or_replayed() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("tool_error");
    let input: std::path::PathBuf = fixture.root.path().join("large.json");
    let arguments: Value = json!({"large": "x".repeat(2 * 1024 * 1024)});
    fs::write(&input, serde_json::to_vec(&arguments).unwrap()).unwrap();
    let output: Output = fixture.run(&["call", "s", "echo", "--input", input.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(7));
    let result: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(result["structuredContent"]["arguments"], arguments);
    assert_eq!(result["isError"], true);
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn scoped_policy_reports_shadowing_and_project_reapproval() {
    use ripmcp::{config::Project, trust::TrustStore};
    let fixture: Fixture = Fixture::new();
    let config: Value = fixture.configure("normal");
    let root: std::path::PathBuf = fixture.root.path().join("project");
    fixture::write(&root.join(".ripmcp/config.json"), &config);
    let project: Project = Project::discover(&root).unwrap().unwrap();
    let trust: TrustStore = TrustStore::new(&fixture.paths).unwrap();
    let output: Output = fixture
        .command(&root, &["disable", "s", "--project"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    trust.approve(&project).unwrap();
    let output: Output = fixture
        .command(&root, &["enable", "s", "--project"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        ripmcp::json::parse(&output.stdout).unwrap()["project_reapproval_required"],
        false
    );
    assert!(
        trust
            .is_approved(&Project::discover(&root).unwrap().unwrap())
            .unwrap()
    );
    let output: Output = fixture
        .command(&root, &["disable", "s", "--user"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        ripmcp::json::parse(&output.stdout).unwrap()["shadowed_by_trusted_project"],
        true
    );
    let output: Output = fixture
        .command(&root, &["disable", "s", "--project"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        ripmcp::json::parse(&output.stdout).unwrap()["project_reapproval_required"],
        true
    );
    assert!(
        !trust
            .is_approved(&Project::discover(&root).unwrap().unwrap())
            .unwrap()
    );
}

fn wait_for(path: &std::path::Path) {
    for _ in 0..500 {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("fixture did not reach expected boundary");
}

#[test]
fn disable_during_discovery_prevents_dispatch() {
    use std::process::{Child, Stdio};
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    fs::write(fixture.root.path().join("server/discovery-wait"), "").unwrap();
    let child: Child = fixture
        .command(
            fixture.root.path(),
            &["call", "s", "echo", "{}", "--timeout", "10"],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&fixture.root.path().join("server/discovering"));
    fixture.ok(&["disable", "s"]);
    fs::remove_file(fixture.root.path().join("server/discovery-wait")).unwrap();
    let output: Output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(!fixture.root.path().join("server/calls").exists());
}

#[test]
fn disable_after_dispatch_does_not_cancel_and_broken_pipe_does_not_replay() {
    use std::process::{Child, Stdio};
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    let child: Child = fixture
        .command(
            fixture.root.path(),
            &[
                "call",
                "s",
                "echo",
                r#"{"wait_file":true}"#,
                "--timeout",
                "10",
            ],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for(&fixture.root.path().join("server/calls"));
    fixture.ok(&["disable", "s"]);
    fs::write(fixture.root.path().join("server/release"), "").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    fixture.ok(&["enable", "s"]);
    let mut child: Child = fixture
        .command(fixture.root.path(), &["call", "s", "echo", "{}"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    assert_eq!(child.wait_with_output().unwrap().status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn discovery_changes_do_not_forget_disabled_names_or_disable_new_tools() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("normal");
    fixture.ok(&["disable", "s", "echo"]);
    let path: std::path::PathBuf = fixture.root.path().join("server/tools.json");
    fs::write(
        &path,
        br#"[{"name":"new","inputSchema":{"type":"object"}}]"#,
    )
    .unwrap();
    assert_eq!(
        fixture.ok(&["tools", "s"])["tools"],
        json!([{"server":"s","name":"new","enabled":true}])
    );
    fixture.ok(&["call", "new", "{}"]);
    fixture.ok(&["stop", "s"]);
    fs::remove_file(path).unwrap();
    assert_eq!(
        fixture.ok(&["tools", "s", "--all"])["tools"][0]["enabled"],
        false
    );
    assert_eq!(fixture.run(&["call", "echo", "{}"]).status.code(), Some(2));
    assert_eq!(fixture.run(&["call", "new", "{}"]).status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
