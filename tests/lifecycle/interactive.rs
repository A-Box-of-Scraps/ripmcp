use super::fixture::Fixture;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStderr, Output, Stdio};
use std::time::{Duration, Instant};

fn spawn(fixture: &Fixture, timeout: &str) -> Child {
    fixture
        .command(
            fixture.root.path(),
            &[
                "call",
                "s",
                "echo",
                "{}",
                "--interactive",
                "--timeout",
                timeout,
            ],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for(fixture: &Fixture, name: &str) {
    let deadline: Instant = Instant::now() + Duration::from_secs(5);
    while !fixture.root.path().join("server").join(name).exists() {
        assert!(Instant::now() < deadline, "missing {name}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn prompt_is_visible_before_completion_and_only_continuation_executes_tool() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("interactive_wait");
    let mut child: Child = spawn(&fixture, "10");
    let stderr: ChildStderr = child.stderr.take().unwrap();
    let reader: std::thread::JoinHandle<String> = std::thread::spawn(move || {
        let mut text: String = String::new();
        for line in BufReader::new(stderr).lines() {
            let line: String = line.unwrap();
            text.push_str(&line);
            text.push('\n');
            if line.starts_with("Waiting for completion") {
                break;
            }
        }
        text
    });
    wait_for(&fixture, "resumed");
    let prompt: String = reader.join().unwrap();
    assert!(prompt.contains("Server \"s\""));
    assert!(prompt.contains("TEST-CODE"));
    assert!(prompt.contains("https://example.com/authorize"));
    assert!(child.try_wait().unwrap().is_none());
    assert!(!fixture.root.path().join("server/executed").exists());
    fs::write(fixture.root.path().join("server/release"), "").unwrap();
    let output: Output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let result: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(result["resultType"], "complete");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private"));
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("server/executed")).unwrap(),
        "once\n"
    );
    assert_eq!(fixture.launches(), 1);
}

#[test]
fn noninteractive_and_unsupported_input_do_not_replay() {
    for (mode, interactive) in [("interactive", false), ("interactive_form", true)] {
        let fixture: Fixture = Fixture::new();
        fixture.configure(mode);
        let mut args: Vec<&str> = vec!["call", "s", "echo", "{}"];
        if interactive {
            args.push("--interactive");
        }
        let output: Output = fixture.run(&args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private"));
        assert!(!fixture.root.path().join("server/resumed").exists());
        let requests: String =
            fs::read_to_string(fixture.root.path().join("server/interactive-requests")).unwrap();
        assert_eq!(requests.lines().count(), 1);
    }
}

#[test]
fn timeout_and_interrupt_cancel_pending_interaction() {
    for interrupt in [false, true] {
        let fixture: Fixture = Fixture::new();
        fixture.configure("interactive_timeout");
        let child: Child = spawn(&fixture, if interrupt { "10" } else { "2" });
        wait_for(&fixture, "resumed");
        if interrupt {
            rustix::process::kill_process(
                rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
                rustix::process::Signal::INT,
            )
            .unwrap();
        }
        let output: Output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let diagnostic: String = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.contains(if interrupt { "cancelled" } else { "timed out" }),
            "{diagnostic}"
        );
        wait_for(&fixture, "cancelled");
        assert!(!fixture.root.path().join("server/executed").exists());
    }
}
