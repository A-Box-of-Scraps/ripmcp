#![cfg(target_os = "linux")]

#[path = "support/fixtures/http.rs"]
mod fixture;
mod support;

use fixture::{Http, Reply};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Output, time::Duration};
use support::Sandbox;

fn configure(sandbox: &Sandbox, definition: Value) {
    let directory: PathBuf = sandbox.root.path().join("XDG_CONFIG_HOME/ripmcp");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("config.json"),
        json!({"schema_version": 1, "servers": {"s": {"definition": definition}}}).to_string(),
    )
    .unwrap();
}

fn remote(url: &str, authentication: &str) -> Value {
    json!({"kind": "remote", "url": url, "transport": "streamable_http", "authentication": authentication})
}

#[test]
fn auth_rejects_local_and_non_oauth_before_side_effects() {
    for definition in [
        remote("https://example.org/mcp", "none"),
        json!({"kind": "local", "runtime": "npx", "package": "fixture", "transport": "stdio"}),
    ] {
        let sandbox: Sandbox = Sandbox::new();
        configure(&sandbox, definition);
        for verb in ["login", "status", "logout"] {
            let output: Output = sandbox.run(&["auth", verb, "s"]);
            assert_eq!(output.status.code(), Some(10));
            assert!(output.stdout.is_empty());
            assert!(!String::from_utf8_lossy(&output.stderr).contains("Open this URL"));
        }
        assert_eq!(std::fs::read_dir(sandbox.root.path()).unwrap().count(), 1);
    }
}

#[test]
fn absent_secret_service_fails_closed_without_network_or_plaintext_files() {
    let sandbox: Sandbox = Sandbox::new();
    configure(&sandbox, remote("https://example.org/mcp", "oauth"));
    for args in [
        &["auth", "login", "s"][..],
        &["auth", "status", "s"],
        &["auth", "logout", "s"],
        &["tools", "s"],
    ] {
        let output: Output = sandbox.run(args);
        assert_eq!(output.status.code(), Some(6), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Secret Service"));
    }
    assert_eq!(std::fs::read_dir(sandbox.root.path()).unwrap().count(), 1);
}

#[test]
fn project_override_requires_trust_before_authentication() {
    let sandbox: Sandbox = Sandbox::new();
    configure(&sandbox, remote("https://example.org/mcp", "oauth"));
    let project: PathBuf = sandbox.root.path().join("project");
    let directory: PathBuf = project.join(".ripmcp");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("config.json"), json!({"schema_version": 1, "servers": {"s": {"definition": remote("https://other.example/mcp", "oauth")}}}).to_string()).unwrap();
    for args in [
        &["auth", "login", "s"][..],
        &["auth", "status", "s"],
        &["auth", "logout", "s"],
        &["tools", "s"],
    ] {
        let output: Output = run_in_project(&sandbox, &project, args);
        assert_eq!(output.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&output.stderr).contains("trust required"));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Secret Service"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Open this URL"));
    }
}

fn run_in_project(sandbox: &Sandbox, project: &std::path::Path, args: &[&str]) -> Output {
    let mut command: std::process::Command =
        std::process::Command::new(env!("CARGO_BIN_EXE_ripmcp"));
    command
        .env_clear()
        .current_dir(project)
        .stdin(std::process::Stdio::null());
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        command.env(key, sandbox.root.path().join(key));
    }
    command.args(args).output().unwrap()
}

fn reply(id: u32, result: Value) -> Reply {
    fixture::json(
        "200 OK",
        &json!({"jsonrpc": "2.0", "id": format!("ripmcp-{id}"), "result": result}).to_string(),
    )
}

fn handshake() -> Reply {
    reply(
        1,
        json!({"resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}}),
    )
}

fn tools() -> Reply {
    reply(
        2,
        json!({"resultType": "complete", "tools": [{"name": "echo", "inputSchema": {"type": "object"}}]}),
    )
}

#[test]
fn remote_calls_preserve_tool_envelopes_without_starting_supervisor() {
    let result: Value = json!({"resultType": "complete", "content": [{"type": "text", "text": "ok"}], "isError": true, "extension": [1, true]});
    let server: Http = Http::new(vec![handshake(), tools(), reply(3, result.clone())]);
    let sandbox: Sandbox = Sandbox::new();
    configure(
        &sandbox,
        remote(&format!("http://{}/mcp", server.address), "none"),
    );
    let output: Output = sandbox.run(&["call", "s", "echo", "{}"]);
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        result
    );
    assert_eq!(std::fs::read_dir(sandbox.root.path()).unwrap().count(), 1);
    for method in ["server/discover", "tools/list", "tools/call"] {
        let request: String = String::from_utf8(
            server
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
        )
        .unwrap();
        assert!(request.contains(method));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }
    server.finish().unwrap();
}

#[test]
fn noninteractive_authentication_failures_never_launch_browser() {
    let server: Http = Http::new(vec![fixture::json("401 Unauthorized", "{}")]);
    let sandbox: Sandbox = Sandbox::new();
    configure(
        &sandbox,
        remote(&format!("http://{}/mcp", server.address), "none"),
    );
    let output: Output = sandbox.run(&["tools", "s"]);
    assert_eq!(output.status.code(), Some(6));
    assert!(output.stdout.is_empty());
    let stderr: String = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("auth login"));
    assert!(!stderr.contains("Open this URL"));
    server.finish().unwrap();
}

#[test]
fn uncertain_remote_calls_are_not_replayed() {
    for failure in [Reply::Close, Reply::Stall, Reply::Disconnect] {
        let server: Http = Http::new(vec![handshake(), tools(), failure]);
        let sandbox: Sandbox = Sandbox::new();
        configure(
            &sandbox,
            remote(&format!("http://{}/mcp", server.address), "none"),
        );
        let output: Output = sandbox.run(&["call", "s", "echo", "{}", "--timeout", "1"]);
        assert!(matches!(output.status.code(), Some(4 | 9)));
        assert!(output.stdout.is_empty());
        let requests: Vec<Vec<u8>> = server.requests.try_iter().collect();
        assert_eq!(requests.iter().filter(|bytes| String::from_utf8_lossy(bytes).contains("\"method\":\"tools/call\"")).count(), 1);
    }
}

#[test]
fn keyring_header_references_require_secure_storage_without_oauth() {
    let sandbox: Sandbox = Sandbox::new();
    let mut definition: Value = remote("https://example.org/mcp", "none");
    definition["headers"] = json!({"Authorization": {"keyring": "external-token"}});
    configure(&sandbox, definition);
    let output: Output = sandbox.run(&["tools", "s"]);
    assert_eq!(output.status.code(), Some(6));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Secret Service"));
    assert!(output.stdout.is_empty());
}

#[test]
fn partial_remote_discovery_preserves_valid_tools_but_cannot_resolve() {
    for args in [&["tools", "s"][..], &["tools"], &["call", "echo", "{}"]] {
        let server: Http = Http::new(vec![
            handshake(),
            reply(
                2,
                json!({"resultType":"complete", "tools":[
                    {"name":"echo", "inputSchema":{"type":"object"}},
                    {"name":"bad", "inputSchema":{"type":"object", "properties":{"token":{"type":"string", "x-mcp-header":""}}}}
                ]}),
            ),
        ]);
        let sandbox: Sandbox = Sandbox::new();
        configure(
            &sandbox,
            remote(&format!("http://{}/mcp", server.address), "none"),
        );
        let output: Output = sandbox.run(args);
        assert_eq!(output.status.code(), Some(8), "{output:?}");
        let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
        assert_eq!(
            value["tools"],
            json!([{"server":"s", "name":"echo", "enabled":true}])
        );
        assert_eq!(value["errors"][0]["tool"], "bad");
        assert_eq!(server.requests.try_iter().count(), 2);
        server.finish().unwrap();
    }
}
