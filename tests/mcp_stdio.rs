#![cfg(target_os = "linux")]

use ripmcp::deadline::Deadline;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::mcp::{
    CancellationToken, Client, Discovery, Limits, Operation, StderrLog, StdioOptions, ToolResult,
};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn operation() -> Operation {
    timed(Duration::from_secs(3))
}

fn timed(duration: Duration) -> Operation {
    Operation::new(Deadline::new(duration), CancellationToken::new())
}

fn options(mode: &str, root: &TempDir) -> StdioOptions {
    let mut options: StdioOptions = StdioOptions::new("/usr/bin/python3");
    options.args = vec![
        "-u".into(),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/fixtures/mcp_stdio.py"
        )
        .into(),
        mode.into(),
        root.path().as_os_str().into(),
    ];
    options.cwd = Some(root.path().to_owned());
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        options
            .env
            .insert(name.into(), root.path().join(name).into_os_string());
    }
    options
}

async fn client(mode: &str, root: &TempDir) -> Client {
    Client::stdio(options(mode, root), Limits::default(), &operation())
        .await
        .unwrap()
}

async fn wait_for(path: &Path) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

fn requests(root: &TempDir) -> Vec<Value> {
    std::fs::read_to_string(root.path().join("requests"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn fragmented_discovery_and_lossless_calls() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let client: Client = client("fragmented", &root).await;
    let discovery: Discovery = client.discover_tools(&operation()).await.unwrap();
    discovery.require_complete().unwrap();
    assert_eq!(discovery.tools.len(), 2);
    assert_eq!(discovery.tools[0].envelope()["vendor"]["retained"], true);
    let result: ToolResult = client
        .call("echo", json!({"hello": [1, null, "world"]}), &operation())
        .await
        .unwrap();
    assert_eq!(result.envelope()["content"].as_array().unwrap().len(), 5);
    assert_eq!(
        result.envelope()["structuredContent"],
        json!({"hello": [1, null, "world"]})
    );
    assert_eq!(
        result.envelope()["extension"]["large"].to_string(),
        "1234567890123456789012345678901234567890"
    );
    let serialized: Value = serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(&serialized, result.envelope());
    let log: StderrLog = client.stderr_log().unwrap();
    assert!(log.redacted_bytes > 0);
    assert!(!serde_json::to_string(&log).unwrap().contains("secret"));
    assert_eq!(client.server_info()["vendor"], "retained");
    client.shutdown().await;
    assert!(
        requests(&root)
            .iter()
            .all(|request| request["method"] != "initialize")
    );
}

#[tokio::test]
async fn negotiation_framing_and_capability_failures_are_explicit() {
    for (mode, kind) in [
        ("version", ErrorKind::Protocol),
        ("malformed", ErrorKind::Protocol),
        ("unterminated", ErrorKind::Protocol),
        ("wrong_id", ErrorKind::Protocol),
        ("server_request", ErrorKind::Protocol),
        ("no_tools", ErrorKind::Unsupported),
        ("exit", ErrorKind::Connection),
    ] {
        let root: TempDir = tempfile::tempdir().unwrap();
        let error: Error = Client::stdio(options(mode, &root), Limits::default(), &operation())
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind, kind, "{mode}");
        assert!(!error.to_string().contains("secret"));
    }
}

#[tokio::test]
async fn out_of_order_concurrent_ids_do_not_cross_deliver() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let client: Client = client("concurrent", &root).await;
    let operation: Operation = operation();
    let (first, second): (Result<ToolResult, Error>, Result<ToolResult, Error>) = tokio::join!(
        client.call("echo", json!({"id": 1}), &operation),
        client.call("echo", json!({"id": 2}), &operation)
    );
    assert_eq!(first.unwrap().envelope()["structuredContent"]["id"], 1);
    assert_eq!(second.unwrap().envelope()["structuredContent"]["id"], 2);
    client.shutdown().await;
}

#[tokio::test]
async fn cancellation_keeps_server_reusable_and_does_not_retry() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let client: Client = client("cancel", &root).await;
    let token: CancellationToken = CancellationToken::new();
    let operation: Operation = Operation::new(Deadline::new(Duration::from_secs(3)), token.clone());
    let (result, ()): (Result<ToolResult, Error>, ()) =
        tokio::join!(client.call("echo", json!({}), &operation), async {
            wait_for(&root.path().join("called")).await;
            token.cancel();
        });
    assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled);
    wait_for(&root.path().join("cancelled")).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("called")).unwrap(),
        "1"
    );
    let result: ToolResult = client
        .call(
            "echo",
            json!({"again": true}),
            &timed(Duration::from_secs(3)),
        )
        .await
        .unwrap();
    assert_eq!(result.envelope()["structuredContent"]["again"], true);
    client.shutdown().await;
}

#[tokio::test]
async fn deadlines_cover_all_pages_and_cancel_calls() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let slow: Client = client("slow_discovery", &root).await;
    let error: Error = slow
        .discover_tools(&timed(Duration::from_millis(100)))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Timeout);
    slow.shutdown().await;
    let root: TempDir = tempfile::tempdir().unwrap();
    let stalled: Client = client("cancel", &root).await;
    let error: Error = stalled
        .call("echo", json!({}), &timed(Duration::from_millis(100)))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Timeout);
    wait_for(&root.path().join("cancelled")).await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("called")).unwrap(),
        "1"
    );
    stalled.shutdown().await;
}

#[tokio::test]
async fn tool_failures_keep_envelopes_and_protocol_failures_do_not() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let failed: Client = client("tool_error", &root).await;
    assert!(
        failed
            .call("echo", json!({}), &operation())
            .await
            .unwrap()
            .is_error()
    );
    failed.shutdown().await;
    for (mode, kind) in [
        ("protocol_error", ErrorKind::Protocol),
        ("input_required", ErrorKind::Unsupported),
        ("drop_call", ErrorKind::Connection),
    ] {
        let root: TempDir = tempfile::tempdir().unwrap();
        let failed: Client = client(mode, &root).await;
        let error: Error = failed
            .call("echo", json!({}), &operation())
            .await
            .unwrap_err();
        assert_eq!(error.kind, kind);
        assert!(!error.to_string().contains("secret"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("called")).unwrap(),
            "1"
        );
        failed.shutdown().await;
    }
}

#[tokio::test]
async fn incomplete_discovery_and_limits_never_become_empty_success() {
    for mode in ["list_failure", "cycle"] {
        let root: TempDir = tempfile::tempdir().unwrap();
        let client: Client = client(mode, &root).await;
        assert_eq!(
            client.discover_tools(&operation()).await.unwrap_err().kind,
            ErrorKind::Protocol
        );
        client.shutdown().await;
    }
    let root: TempDir = tempfile::tempdir().unwrap();
    let limits: Limits = Limits {
        frame_bytes: 512,
        ..Limits::default()
    };
    let error: Error = Client::stdio(options("oversize", &root), limits, &operation())
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ErrorKind::Protocol);
    let client: Client = Client::stdio(
        options("normal", &root),
        Limits {
            pages: 1,
            ..Limits::default()
        },
        &operation(),
    )
    .await
    .unwrap();
    assert_eq!(
        client.discover_tools(&operation()).await.unwrap_err().kind,
        ErrorKind::Protocol
    );
    client.shutdown().await;
}

#[tokio::test]
async fn logs_are_drained_and_shutdown_escalates_and_reaps() {
    for mode in ["stderr_flood", "immortal"] {
        let root: TempDir = tempfile::tempdir().unwrap();
        let client: Client = client(mode, &root).await;
        let pid: String = std::fs::read_to_string(root.path().join("pid")).unwrap();
        client.shutdown().await;
        assert!(!Path::new("/proc").join(pid).exists());
        assert_eq!(
            client.discover_tools(&operation()).await.unwrap_err().kind,
            ErrorKind::Connection
        );
    }
}

#[tokio::test]
async fn sigint_cancels_in_an_isolated_process() {
    if std::env::var_os("RIPMCP_SIGNAL_CHILD").is_none() {
        let output: std::process::Output =
            std::process::Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env("RIPMCP_SIGNAL_CHILD", "1")
                .args(["--exact", "sigint_cancels_in_an_isolated_process"])
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    let root: TempDir = tempfile::tempdir().unwrap();
    let client: Client = client("cancel", &root).await;
    let signal: ripmcp::mcp::SignalCancellation =
        ripmcp::mcp::SignalCancellation::install().unwrap();
    let operation: Operation =
        Operation::new(Deadline::new(Duration::from_secs(3)), signal.token());
    let (result, ()): (Result<ToolResult, Error>, ()) =
        tokio::join!(client.call("echo", json!({}), &operation), async {
            wait_for(&root.path().join("called")).await;
            rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::INT)
                .unwrap();
        });
    assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled);
    wait_for(&root.path().join("cancelled")).await;
    client.shutdown().await;
}

#[tokio::test]
async fn initialization_deadline_reaps_and_precancellation_never_spawns() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let error: Error = Client::stdio(
        options("stall_init", &root),
        Limits::default(),
        &timed(Duration::from_millis(100)),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.kind, ErrorKind::Timeout);
    let pid: String = std::fs::read_to_string(root.path().join("pid")).unwrap();
    assert!(!Path::new("/proc").join(pid).exists());
    let root: TempDir = tempfile::tempdir().unwrap();
    let token: CancellationToken = CancellationToken::new();
    token.cancel();
    let operation: Operation = Operation::new(Deadline::new(Duration::from_secs(1)), token);
    assert_eq!(
        Client::stdio(options("normal", &root), Limits::default(), &operation)
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::Cancelled
    );
    assert!(!root.path().join("pid").exists());
}

#[tokio::test]
async fn queue_wait_uses_deadline_without_sending_another_request() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let client: Client = Client::stdio(
        options("cancel", &root),
        Limits {
            in_flight: 1,
            ..Limits::default()
        },
        &operation(),
    )
    .await
    .unwrap();
    let token: CancellationToken = CancellationToken::new();
    let operation: Operation = Operation::new(Deadline::new(Duration::from_secs(3)), token.clone());
    let (result, ()): (Result<ToolResult, Error>, ()) =
        tokio::join!(client.call("echo", json!({}), &operation), async {
            wait_for(&root.path().join("called")).await;
            assert_eq!(
                client
                    .discover_tools(&timed(Duration::from_millis(25)))
                    .await
                    .unwrap_err()
                    .kind,
                ErrorKind::Timeout
            );
            assert_eq!(requests(&root).len(), 3);
            token.cancel();
        });
    assert_eq!(result.unwrap_err().kind, ErrorKind::Cancelled);
    client.shutdown().await;
}
