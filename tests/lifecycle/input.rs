use super::fixture::Fixture;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::process::{Child, ChildStdin, Output, Stdio};
use std::time::{Duration, Instant};

#[test]
fn file_and_stdin_calls_preserve_complete_envelopes_and_exact_json() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    let input: &str = r#"{"n":1234567890123456789012345678901234567890,"$serde_json::private::Number":"literal"}"#;
    let path: std::path::PathBuf = fixture.root.path().join("arguments.json");
    fs::write(&path, input).unwrap();
    let file: Value = fixture.ok(&["call", "s", "echo", "--input", path.to_str().unwrap()]);
    let mut child: Child = fixture
        .command(fixture.root.path(), &["call", "s", "echo", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin: ChildStdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    let output: Output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let pipe: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(pipe, file);
    assert_eq!(
        pipe["structuredContent"]["arguments"],
        ripmcp::json::parse(input.as_bytes()).unwrap()
    );
    assert!(pipe.get("schema_version").is_none());
}

#[test]
fn stalled_stdin_times_out_without_starting_a_supervisor() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    let mut child: Child = fixture
        .command(
            fixture.root.path(),
            &["--timeout", "1", "call", "s", "echo", "--input", "-"],
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _stdin: ChildStdin = child.stdin.take().unwrap();
    let until: Instant = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(9));
            break;
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("stdin ignored deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!fixture.runtime().exists());
}

#[test]
fn tool_policy_and_tool_reported_failure_keep_their_distinct_exit_codes() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("tool_error");
    let output: Output = fixture.run(&["call", "s", "echo", "{}"]);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        ripmcp::json::parse(&output.stdout).unwrap()["isError"],
        true
    );
    config["servers"]["s"]["disabled_tools"] = json!(["echo"]);
    fixture.write_user(&config);
    assert_eq!(
        fixture.run(&["call", "s", "echo", "{}"]).status.code(),
        Some(3)
    );
    assert_eq!(fixture.ok(&["tools", "s"])["tools"], json!([]));
    assert_eq!(
        fixture.ok(&["tools", "s", "--all"])["tools"][0]["name"],
        "echo"
    );
}
