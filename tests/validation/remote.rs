use super::fixture::Fixture;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
#[allow(dead_code)]
#[path = "../support/fixtures/http.rs"]
mod http;
use http::{Http, Reply};

fn reply(id: u32, result: Value) -> Reply {
    http::json(
        "200 OK",
        &json!({"jsonrpc":"2.0", "id":format!("ripmcp-{id}"), "result":result}).to_string(),
    )
}

fn discovery() -> Vec<Reply> {
    vec![
        reply(
            1,
            json!({"resultType":"complete", "supportedVersions":["2026-07-28"], "capabilities":{"tools":{}}}),
        ),
        reply(
            2,
            json!({"resultType":"complete", "tools":[{"name":"echo", "inputSchema":{"type":"object"}}]}),
        ),
    ]
}

#[test]
fn registered_remote_lifecycle_is_passive_until_discovery_and_never_managed() {
    let fixture: Fixture = Fixture::new();
    let mut replies: Vec<Reply> = discovery();
    replies.extend(discovery());
    replies.push(reply(
        3,
        json!({"resultType":"complete", "content":[], "extension":{"kept":true}}),
    ));
    let server: Http = Http::new(replies);
    let path: PathBuf = fixture.import(&json!({"definition":{"kind":"remote", "url":format!("http://{}/mcp", server.address), "transport":"streamable_http"}}));
    let _: Value = fixture.ok(&[
        "install",
        "s",
        "--config",
        path.to_str().unwrap(),
        "--skip-verify",
    ]);
    assert_eq!(
        fixture.ok(&["servers"])["servers"][0]["process_state"],
        "not_managed"
    );
    for command in ["start", "stop"] {
        assert_eq!(fixture.run(&[command, "s"]).status.code(), Some(10));
    }
    assert!(server.requests.try_recv().is_err());
    assert_eq!(fixture.ok(&["tools", "s"])["tools"][0]["name"], "echo");
    assert_eq!(
        fixture.ok(&["call", "s", "echo", "{}"])["extension"]["kept"],
        true
    );
    let _: Value = fixture.ok(&["uninstall", "s", "--clean", "-y"]);
    assert_eq!(fixture.ok(&["servers"])["servers"], json!([]));
    for method in [
        "server/discover",
        "tools/list",
        "server/discover",
        "tools/list",
        "tools/call",
    ] {
        let request: Vec<u8> = server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(
            String::from_utf8(request)
                .unwrap()
                .contains(&format!("\"method\":\"{method}\""))
        );
    }
    server.finish().unwrap();
    assert!(fixture.log().is_empty());
    assert!(path.exists());
}
