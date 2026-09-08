use super::fixture::Fixture;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Output, time::Duration};
#[path = "../support/fixtures/http.rs"]
mod http;
use http::{Http, Reply};

fn reply(id: u32, result: Value) -> Reply {
    http::json(
        "200 OK",
        &json!({"jsonrpc":"2.0", "id":format!("ripmcp-{id}"), "result":result}).to_string(),
    )
}
fn definition(fixture: &Fixture, url: &str, auth: &str) -> PathBuf {
    fixture.import(&json!({"definition": {"kind":"remote", "url":url, "transport":"streamable_http", "authentication":auth}}))
}

#[test]
fn remote_verifies_two_requests_without_tool_calls() {
    let fixture: Fixture = Fixture::new();
    let http: Http = Http::new(vec![
        reply(
            1,
            json!({"resultType":"complete", "supportedVersions":["2026-07-28"], "capabilities":{"tools":{}}}),
        ),
        reply(2, json!({"resultType":"complete", "tools":[]})),
    ]);
    let url: String = format!("http://{}/secret?token=do-not-report", http.address);
    let path: PathBuf = definition(&fixture, &url, "none");
    let report: Value = fixture.ok(&["install", "s", "--config", path.to_str().unwrap()]);
    assert_eq!(report["installation"]["verification"], "verified");
    assert!(!report.to_string().contains("do-not-report"));
    for method in ["server/discover", "tools/list"] {
        let bytes: Vec<u8> = http.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let request: String = String::from_utf8(bytes).unwrap();
        assert!(request.contains(method));
        assert!(!request.contains("tools/call"));
    }
    http.finish().unwrap();
}

#[test]
fn auth_failure_is_actionable_before_registration_and_skip_is_offline() {
    let fixture: Fixture = Fixture::new();
    let http: Http = Http::new(vec![http::json("401 Unauthorized", "{}")]);
    let path: PathBuf = definition(&fixture, &format!("http://{}/mcp", http.address), "none");
    let output: Output = fixture.run(&["install", "s", "--config", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(6), "{output:?}");
    let diagnostic: String = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("--skip-verify"));
    assert!(diagnostic.contains("auth login"));
    assert!(diagnostic.contains("tools <server>"));
    assert!(!fixture.config().exists());
    let report: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert_eq!(report["installation"]["verification"], "unverified");
    assert!(
        !http
            .requests
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_empty()
    );
    http.finish().unwrap();
}

#[test]
fn remote_skip_does_not_access_keyring_or_connect() {
    let fixture: Fixture = Fixture::new();
    let path: PathBuf = definition(&fixture, "https://example.invalid/mcp", "oauth");
    let _: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert!(fixture.log().is_empty());
}

#[test]
fn transport_failure_and_timeout_leave_no_registration() {
    for reply in [Reply::Close, Reply::Disconnect, Reply::Stall] {
        let fixture: Fixture = Fixture::new();
        let http: Http = Http::new(vec![reply]);
        let path: PathBuf = definition(&fixture, &format!("http://{}/mcp", http.address), "none");
        let output: Output = fixture.run(&[
            "install",
            "s",
            "--config",
            path.to_str().unwrap(),
            "--timeout",
            "1",
        ]);
        assert!(!output.status.success());
        assert!(!fixture.config().exists());
    }
}

#[test]
fn skip_verify_still_validates_transport_and_header_ownership() {
    let fixture: Fixture = Fixture::new();
    for (url, auth, headers) in [
        ("http://example.invalid/mcp", "none", json!({})),
        (
            "http://localhost/mcp",
            "none",
            json!({"x-api-key":{"env":"KEY"}}),
        ),
        (
            "https://example.invalid/mcp",
            "oauth",
            json!({"Authorization":{"env":"KEY"}}),
        ),
        (
            "https://example.invalid/mcp",
            "none",
            json!({"Mcp-Version":{"env":"KEY"}}),
        ),
        (
            "https://example.invalid/mcp",
            "none",
            json!({"x-api-key":{"env":"KEY"}, "X-API-KEY":{"env":"KEY"}}),
        ),
    ] {
        let path: PathBuf = fixture.import(&json!({"definition": {"kind":"remote", "url":url, "transport":"streamable_http", "authentication":auth, "headers":headers}}));
        let output: Output = fixture.run(&[
            "install",
            "s",
            "--config",
            path.to_str().unwrap(),
            "--skip-verify",
        ]);
        assert_eq!(output.status.code(), Some(3));
        assert!(!fixture.config().exists());
    }
}
