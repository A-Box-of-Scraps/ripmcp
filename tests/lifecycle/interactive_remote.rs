use super::fixture::Fixture;
#[expect(dead_code)]
#[path = "../support/fixtures/http.rs"]
mod http;
use http::{Http, Reply};
use serde_json::{Value, json};
use std::process::Output;
use std::time::Duration;

fn response(id: u32, result: Value) -> Reply {
    http::json(
        "200 OK",
        &json!({"jsonrpc": "2.0", "id": format!("ripmcp-{id}"), "result": result}).to_string(),
    )
}

#[test]
fn remote_interactive_call_preserves_arguments_and_request_state() {
    let fixture: Fixture = Fixture::new();
    let http: Http = Http::new(vec![
        response(
            1,
            json!({"resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}}),
        ),
        response(
            2,
            json!({"resultType": "complete", "tools": [{"name": "echo", "inputSchema": {"type": "object"}}]}),
        ),
        response(
            3,
            json!({"resultType": "input_required", "requestState": "opaque", "inputRequests": {
                "login": {"method": "elicitation/create", "params": {"mode": "url", "url": "https://example.com/login", "message": "Authorize"}}
            }}),
        ),
        response(
            4,
            json!({"resultType": "complete", "content": [], "structuredContent": {"ok": true}}),
        ),
    ]);
    fixture.write_user(&json!({"schema_version": 1, "servers": {"s": {"definition": {
        "kind": "remote", "url": format!("http://{}/mcp", http.address), "transport": "streamable_http", "authentication": "none"
    }}}}));
    let output: Output = fixture.run(&["call", "s", "echo", "{\"x\":1}", "--interactive"]);
    assert!(output.status.success(), "{output:?}");
    let result: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(result["structuredContent"]["ok"], true);
    assert!(String::from_utf8_lossy(&output.stderr).contains("https://example.com/login"));
    for index in 1..=4 {
        let bytes: Vec<u8> = http.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        let text: String = String::from_utf8(bytes).unwrap();
        let (_, body): (&str, &str) = text.split_once("\r\n\r\n").unwrap();
        let request: Value = ripmcp::json::parse(body.as_bytes()).unwrap();
        assert_eq!(
            request["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"],
            json!({"elicitation": {"url": {}}})
        );
        if index == 4 {
            assert_eq!(request["params"]["arguments"], json!({"x": 1}));
            assert_eq!(
                request["params"]["inputResponses"],
                json!({"login": {"action": "accept"}})
            );
            assert_eq!(request["params"]["requestState"], "opaque");
        }
    }
    http.finish().unwrap();
}
