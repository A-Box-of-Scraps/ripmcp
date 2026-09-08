#![cfg(target_os = "linux")]

#[path = "support/fixtures/http.rs"]
mod http;
#[path = "support/fixtures/process.rs"]
mod process;

use http::{Http, Reply};
use process::{Process, Script};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::process::{ChildStdin, Output};
use std::time::Duration;

#[test]
fn fake_runtimes_capture_literal_arguments_and_failures() {
    for name in ["npx", "uvx", "docker"] {
        let runtime: Script = Script::new(
            name,
            "printf '%s\\n' \"$@\"; printf 'fixture diagnostic\\n' >&2; exit 23",
        );
        let output: Output = runtime
            .spawn(&["package", "space argument", "$(not-executed)"])
            .finish(Duration::from_secs(2))
            .unwrap();
        assert_eq!(output.status.code(), Some(23));
        assert_eq!(output.stdout, b"package\nspace argument\n$(not-executed)\n");
        assert_eq!(output.stderr, b"fixture diagnostic\n");
    }
}

#[test]
fn stdio_scaffold_exchanges_lines_and_keeps_logs_separate() {
    let server: Script = Script::new(
        "stdio",
        "while IFS= read -r request; do\n printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}'\n printf 'fixture log\\n' >&2\ndone",
    );
    let mut process: Process = server.spawn(&[]);
    let mut stdin: ChildStdin = process.child().stdin.take().unwrap();
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n")
        .unwrap();
    drop(stdin);
    let output: Output = process.finish(Duration::from_secs(2)).unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["tools"], serde_json::json!([]));
    assert_eq!(output.stderr, b"fixture log\n");
}

#[test]
fn stdio_scaffold_injects_malformed_frames_and_early_exit() {
    for body in ["printf 'not-json\\n'", "exit 17"] {
        let server: Script = Script::new("stdio", body);
        let output: Output = server.spawn(&[]).finish(Duration::from_secs(2)).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_err());
    }
}

#[test]
fn stalled_process_is_killed_and_reaped_on_timeout() {
    let server: Script = Script::new("stdio", "IFS= read -r request");
    let error: io::Error = server
        .spawn(&[])
        .finish(Duration::from_millis(10))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

fn connect(server: &Http, request: &[u8]) -> TcpStream {
    let mut stream: TcpStream =
        TcpStream::connect_timeout(&server.address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(request).unwrap();
    stream
}

#[test]
fn http_oauth_scaffold_records_discovery_and_token_requests() {
    let server: Http = Http::new(vec![
        http::json("200 OK", "{\"token_endpoint\":\"/token\"}"),
        http::json("400 Bad Request", "{\"error\":\"invalid_grant\"}"),
    ]);
    for request in [
        "GET /.well-known/oauth-authorization-server HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "POST /token HTTP/1.1\r\nHost: localhost\r\nContent-Length: 6\r\n\r\ncode=x",
    ] {
        let mut stream: TcpStream = connect(&server, request.as_bytes());
        let mut response: String = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 "));
        assert_eq!(
            server
                .requests
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            request.as_bytes()
        );
    }
    server.finish().unwrap();
}

#[test]
fn http_scaffold_injects_close_malformed_response_and_timeout() {
    for reply in [
        Reply::Close,
        Reply::Bytes(b"broken\r\n".to_vec()),
        Reply::Stall,
    ] {
        let stalled = matches!(reply, Reply::Stall);
        let server: Http = Http::new(vec![reply]);
        let mut stream: TcpStream = connect(&server, b"GET / HTTP/1.1\r\n\r\n");
        server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut response: Vec<u8> = Vec::new();
        let result: io::Result<usize> = stream.read_to_end(&mut response);
        if stalled {
            assert!(matches!(
                result.unwrap_err().kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ));
        } else {
            result.unwrap();
            assert!(!response.starts_with(b"HTTP/1.1"));
            server.finish().unwrap();
        }
    }
}

#[test]
fn http_scaffold_observes_stream_cancellation() {
    let server: Http = Http::new(vec![Reply::Disconnect]);
    let mut stream: TcpStream = connect(&server, b"GET / HTTP/1.1\r\n\r\n");
    server
        .requests
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let mut buffer: [u8; 128] = [0; 128];
    assert!(stream.read(&mut buffer).unwrap() > 0);
    drop(stream);
    assert_eq!(
        server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        b"disconnected"
    );
    server.finish().unwrap();
}
