#![cfg(target_os = "linux")]

#[path = "support/fixtures/http.rs"]
mod fixture;

use fixture::{Http, Reply};
use reqwest::header::{HeaderMap, HeaderValue};
use ripmcp::deadline::Deadline;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::mcp::{
    AuthenticationProvider, AuthorizationFuture, CancellationToken, ChallengeFuture, Client,
    Discovery, HttpOptions, Limits, Operation, ToolResult,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

fn operation() -> Operation {
    timed(Duration::from_secs(3))
}

fn timed(duration: Duration) -> Operation {
    Operation::new(Deadline::new(duration), CancellationToken::new())
}

fn endpoint(server: &Http) -> Url {
    Url::parse(&format!("http://{}/mcp", server.address)).unwrap()
}

async fn client(server: &Http) -> Client {
    Client::http(
        HttpOptions::new(endpoint(server)),
        Limits::default(),
        &operation(),
    )
    .await
    .unwrap()
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

fn tool(name: &str, schema: Value) -> Value {
    json!({"name": name, "inputSchema": schema, "vendor": {"keep": true}})
}

fn tools() -> Reply {
    reply(
        2,
        json!({"resultType": "complete", "tools": [tool("echo", json!({"type": "object"}))]}),
    )
}

fn captured(server: &Http) -> (String, Value) {
    let bytes: Vec<u8> = server
        .requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let text: String = String::from_utf8(bytes).unwrap();
    let (headers, body): (&str, &str) = text.split_once("\r\n\r\n").unwrap();
    (
        headers.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap(),
    )
}

#[tokio::test]
async fn required_metadata_and_lossless_json_are_preserved() {
    let result: Value = json!({"resultType": "complete", "content": [{"type": "text", "text": "ok"}], "structuredContent": [1, "array", null], "isError": true, "extra": {"kept": true}});
    let server: Http = Http::new(vec![handshake(), tools(), reply(3, result.clone())]);
    let client: Client = client(&server).await;
    let output: ToolResult = client
        .call("echo", json!({"n": 1}), &operation())
        .await
        .unwrap();
    assert_eq!(output.envelope(), &result);
    assert!(output.is_error());
    assert_eq!(output.into_envelope(), result);
    for method in ["server/discover", "tools/list", "tools/call"] {
        let (headers, body): (String, Value) = captured(&server);
        assert!(headers.starts_with("post /mcp http/1.1"));
        assert!(headers.contains("accept: application/json, text/event-stream"));
        assert!(headers.contains("content-type: application/json"));
        assert!(headers.contains("mcp-protocol-version: 2026-07-28"));
        assert!(headers.contains(&format!("mcp-method: {method}")));
        assert!(!headers.contains("mcp-session-id"));
        assert!(!headers.contains("last-event-id"));
        assert_eq!(
            body["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"],
            json!({})
        );
        assert_eq!(
            body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
            "2026-07-28"
        );
    }
    client.shutdown().await;
    server.finish().unwrap();
}

#[tokio::test]
async fn mirrors_nested_annotations_and_encodes_unsafe_values() {
    let schema: Value = json!({"type": "object", "properties": {
        "nested": {"type": "object", "properties": {"tenant": {"type": "string", "x-mcp-header": "Tenant"}}},
        "number": {"type": "integer", "x-mcp-header": "Number"},
        "flag": {"type": "boolean", "x-mcp-header": "Flag"},
        "absent": {"type": "string", "x-mcp-header": "Absent"},
        "null": {"type": "string", "x-mcp-header": "Null"},
        "sentinel": {"type": "string", "x-mcp-header": "Sentinel"}
    }});
    let server: Http = Http::new(vec![
        handshake(),
        reply(
            2,
            json!({"resultType": "complete", "tools": [tool(" spaced ", schema)]}),
        ),
        reply(3, json!({"resultType": "complete", "content": []})),
    ]);
    let client: Client = client(&server).await;
    client.call(" spaced ", json!({"nested": {"tenant": "a\r\nb"}, "number": 42.0, "flag": true, "null": null, "sentinel": "=?base64?literal?="}), &operation()).await.unwrap();
    captured(&server);
    captured(&server);
    let (headers, _): (String, Value) = captured(&server);
    assert!(headers.contains("mcp-name: =?base64?ihnwywnlzca=?="));
    assert!(headers.contains("mcp-param-tenant: =?base64?yq0kyg==?="));
    assert!(headers.contains("mcp-param-number: 42\r\n"));
    assert!(headers.contains("mcp-param-flag: true\r\n"));
    assert!(headers.contains("mcp-param-sentinel: =?base64?"));
    assert!(!headers.contains("mcp-param-absent"));
    assert!(!headers.contains("mcp-param-null"));
    server.finish().unwrap();
}

#[tokio::test]
async fn invalid_annotations_exclude_only_invalid_tools_and_report_incompleteness() {
    let bad: Vec<Value> = vec![
        json!({"type": "object", "properties": {"a": {"type": "string", "x-mcp-header": ""}}}),
        json!({"type": "object", "properties": {"a": {"type": "string", "x-mcp-header": "Bad\r\nName"}}}),
        json!({"type": "object", "properties": {"a": {"type": "number", "x-mcp-header": "N"}}}),
        json!({"type": "object", "properties": {"a": {"type": "string", "x-mcp-header": "N"}, "b": {"type": "string", "x-mcp-header": "n"}}}),
        json!({"type": "object", "oneOf": [{"properties": {"a": {"type": "string", "x-mcp-header": "N"}}}]}),
        json!({"type": "object", "properties": {"a": {"type": "array", "items": {"type": "string", "x-mcp-header": "N"}}}}),
    ];
    let mut definitions: Vec<Value> = vec![tool("valid", json!({"type": "object"}))];
    for (index, schema) in bad.into_iter().enumerate() {
        definitions.push(tool(&format!("bad{index}"), schema));
    }
    let server: Http = Http::new(vec![
        handshake(),
        reply(2, json!({"resultType": "complete", "tools": definitions})),
    ]);
    let client: Client = client(&server).await;
    let discovery: Discovery = client.discover_tools(&operation()).await.unwrap();
    assert_eq!(discovery.tools.len(), 1);
    assert_eq!(discovery.rejected_tools.len(), 6);
    assert_eq!(
        discovery.require_complete().unwrap_err().kind,
        ErrorKind::PartialFailure
    );
    server.finish().unwrap();
}

#[tokio::test]
async fn unsafe_integer_is_rejected_before_invocation() {
    let server: Http = Http::new(vec![
        handshake(),
        reply(
            2,
            json!({"resultType": "complete", "tools": [tool("echo", json!({"type": "object", "properties": {"n": {"type": "integer", "x-mcp-header": "N"}}}))]}),
        ),
    ]);
    let client: Client = client(&server).await;
    assert_eq!(
        client
            .call("echo", json!({"n": 9007199254740992_u64}), &operation())
            .await
            .unwrap_err()
            .kind,
        ErrorKind::Usage
    );
    server.finish().unwrap();
}

#[tokio::test]
async fn sse_accepts_comments_notifications_and_multiline_messages() {
    let body: String = format!(
        "\u{feff}: heartbeat\r\n\r\nevent: message\r\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{{}}}}\r\n\r\ndata: {{\"jsonrpc\":\"2.0\",\r\ndata: \"id\":\"ripmcp-3\",\"result\":{{\"resultType\":\"complete\",\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}}}}\r\n\r\n",
        "unicode \u{2603}"
    );
    let response: Reply = Reply::Bytes(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nConnection: close\r\n\r\n{body}").into_bytes());
    let server: Http = Http::new(vec![handshake(), tools(), response]);
    let client: Client = client(&server).await;
    let result: ToolResult = client.call("echo", json!({}), &operation()).await.unwrap();
    assert_eq!(result.envelope()["content"][0]["text"], "unicode \u{2603}");
    server.finish().unwrap();
}

#[tokio::test]
async fn http_failures_authentication_and_mismatches_are_safe() {
    let failures: Vec<(Reply, ErrorKind)> = vec![
        (
            fixture::json("401 Unauthorized", "secret-token"),
            ErrorKind::Authentication,
        ),
        (
            fixture::json("403 Forbidden", "secret-token"),
            ErrorKind::Authentication,
        ),
        (
            fixture::json(
                "400 Bad Request",
                &json!({"jsonrpc": "2.0", "error": {"code": -32022, "message": "secret-token"}})
                    .to_string(),
            ),
            ErrorKind::Protocol,
        ),
        (reply(999, json!({})), ErrorKind::Protocol),
        (fixture::json("200 OK", "malformed"), ErrorKind::Protocol),
        (
            Reply::Bytes(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec()),
            ErrorKind::Connection,
        ),
        (
            Reply::Bytes(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".to_vec()),
            ErrorKind::Protocol,
        ),
        (Reply::Close, ErrorKind::Connection),
    ];
    for (reply, kind) in failures {
        let server: Http = Http::new(vec![reply]);
        let error: Error = Client::http(
            HttpOptions::new(endpoint(&server)),
            Limits::default(),
            &operation(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.kind, kind);
        assert!(!error.to_string().contains("secret-token"));
        server.finish().unwrap();
    }
}

#[tokio::test]
async fn interrupted_call_is_not_replayed_and_http_deadline_is_enforced() {
    for reply in [Reply::Close, Reply::Stall] {
        let expected: ErrorKind = if matches!(reply, Reply::Stall) {
            ErrorKind::Timeout
        } else {
            ErrorKind::Connection
        };
        let server: Http = Http::new(vec![handshake(), tools(), reply]);
        let client: Client = client(&server).await;
        let error: Error = client
            .call("echo", json!({}), &timed(Duration::from_millis(100)))
            .await
            .unwrap_err();
        assert_eq!(error.kind, expected);
        for method in ["server/discover", "tools/list", "tools/call"] {
            assert_eq!(captured(&server).1["method"], method);
        }
        assert!(server.requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn redirects_never_contact_the_new_origin() {
    let target: std::net::TcpListener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    target.set_nonblocking(true).unwrap();
    let server: Http = Http::new(vec![Reply::Bytes(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/stolen\r\nContent-Length: 0\r\n\r\n", target.local_addr().unwrap()).into_bytes())]);
    let error: Error = Client::http(
        HttpOptions::new(endpoint(&server)),
        Limits::default(),
        &operation(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.kind, ErrorKind::Connection);
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    server.finish().unwrap();
}

struct TokenProvider;

impl AuthenticationProvider for TokenProvider {
    fn authorization<'a>(&'a self, _: &'a Url, _: &'a Operation) -> AuthorizationFuture<'a> {
        Box::pin(async { Ok(Some(HeaderValue::from_static("Bearer secret-token"))) })
    }
}

#[tokio::test]
async fn endpoints_headers_and_plaintext_credentials_fail_closed() {
    for url in [
        "http://example.com/mcp",
        "https://user:secret@example.com/mcp",
        "https://example.com/mcp#fragment",
        "file:///tmp/mcp",
    ] {
        let error: Error = Client::http(
            HttpOptions::new(Url::parse(url).unwrap()),
            Limits::default(),
            &operation(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.kind, ErrorKind::Configuration);
    }
    let listener: std::net::TcpListener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let url: Url = Url::parse(&format!("http://{}/mcp", listener.local_addr().unwrap())).unwrap();
    let mut options: HttpOptions = HttpOptions::new(url.clone());
    options.authentication = Arc::new(TokenProvider);
    assert_eq!(
        Client::http(options, Limits::default(), &operation())
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::Authentication
    );
    let mut options: HttpOptions = HttpOptions::new(url);
    options.headers = HeaderMap::new();
    options
        .headers
        .insert("mcp-method", HeaderValue::from_static("tools/call"));
    assert_eq!(
        Client::http(options, Limits::default(), &operation())
            .await
            .err()
            .unwrap()
            .kind,
        ErrorKind::Configuration
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

struct ChallengeRecorder {
    expected: Url,
    seen: std::sync::atomic::AtomicUsize,
}

impl AuthenticationProvider for ChallengeRecorder {
    fn authorization<'a>(&'a self, resource: &'a Url, _: &'a Operation) -> AuthorizationFuture<'a> {
        Box::pin(async move {
            assert_eq!(resource, &self.expected);
            Ok(None)
        })
    }

    fn challenged<'a>(
        &'a self,
        resource: &'a Url,
        status: reqwest::StatusCode,
        headers: &'a HeaderMap,
        _: &'a Operation,
    ) -> ChallengeFuture<'a> {
        Box::pin(async move {
            assert_eq!(resource, &self.expected);
            assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
            assert!(
                headers["www-authenticate"]
                    .to_str()
                    .unwrap()
                    .contains("resource_metadata=")
            );
            self.seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        })
    }
}

#[tokio::test]
async fn authentication_challenge_reaches_only_the_bound_provider_without_retry() {
    let server: Http = Http::new(vec![Reply::Bytes(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer resource_metadata=\"https://example.com/protected-resource\"\r\nContent-Length: 0\r\n\r\n".to_vec())]);
    let provider: Arc<ChallengeRecorder> = Arc::new(ChallengeRecorder {
        expected: endpoint(&server),
        seen: std::sync::atomic::AtomicUsize::new(0),
    });
    let mut options: HttpOptions = HttpOptions::new(endpoint(&server));
    options.authentication = provider.clone();
    let error: Error = Client::http(options, Limits::default(), &operation())
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ErrorKind::Authentication);
    assert!(error.to_string().contains("auth login"));
    assert_eq!(provider.seen.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert!(!captured(&server).0.contains("authorization:"));
    server.finish().unwrap();
}

#[tokio::test]
async fn pagination_is_complete_or_an_explicit_failure() {
    let first: Value = json!({"resultType": "complete", "tools": [tool("a", json!({"type": "object"}))], "nextCursor": "opaque"});
    let second: Value =
        json!({"resultType": "complete", "tools": [tool("b", json!({"type": "object"}))]});
    let server: Http = Http::new(vec![handshake(), reply(2, first.clone()), reply(3, second)]);
    let client: Client = client(&server).await;
    assert_eq!(
        client
            .discover_tools(&operation())
            .await
            .unwrap()
            .tools
            .len(),
        2
    );
    captured(&server);
    captured(&server);
    assert_eq!(captured(&server).1["params"]["cursor"], "opaque");
    server.finish().unwrap();
    let server: Http = Http::new(vec![handshake(), reply(2, first), Reply::Close]);
    let client: Client = Client::http(
        HttpOptions::new(endpoint(&server)),
        Limits::default(),
        &operation(),
    )
    .await
    .unwrap();
    assert_eq!(
        client.discover_tools(&operation()).await.unwrap_err().kind,
        ErrorKind::Connection
    );
    server.finish().unwrap();
}

#[tokio::test]
async fn body_and_event_limits_are_explicit_and_unfinished_streams_fail() {
    for (response, kind) in [
        ("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999999999\r\n\r\n".to_owned(), ErrorKind::Protocol),
        (format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: {}", "x".repeat(1025)), ErrorKind::Protocol),
        ("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n: heartbeat\n\n".to_owned(), ErrorKind::Connection),
    ] {
        let server: Http = Http::new(vec![Reply::Bytes(response.into_bytes())]);
        let error: Error = Client::http(HttpOptions::new(endpoint(&server)), Limits { frame_bytes: 1024, ..Limits::default() }, &operation()).await.err().unwrap();
        assert_eq!(error.kind, kind);
        server.finish().unwrap();
    }
}

async fn wait_for_requests(server: &Http, count: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut seen = 0;
        while seen < count {
            if server.requests.try_recv().is_ok() {
                seen += 1;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

async fn wait_for_disconnect(server: &Http) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(message) = server.requests.try_recv() {
                assert_eq!(message, b"disconnected");
                return;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_and_shutdown_close_only_request_streams_without_replay() {
    for shutdown in [false, true] {
        let server: Http = Http::new(vec![handshake(), tools(), Reply::Disconnect]);
        let client: Client = client(&server).await;
        let token: CancellationToken = CancellationToken::new();
        let operation: Operation =
            Operation::new(Deadline::new(Duration::from_secs(3)), token.clone());
        let (result, ()): (Result<ToolResult, Error>, ()) =
            tokio::join!(client.call("echo", json!({}), &operation), async {
                wait_for_requests(&server, 3).await;
                if shutdown {
                    client.shutdown().await;
                } else {
                    token.cancel();
                }
            });
        assert_eq!(
            result.unwrap_err().kind,
            if shutdown {
                ErrorKind::Connection
            } else {
                ErrorKind::Cancelled
            }
        );
        wait_for_disconnect(&server).await;
        assert!(server.requests.try_recv().is_err());
        server.finish().unwrap();
    }
}
