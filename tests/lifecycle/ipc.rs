use super::fixture::Fixture;
use ripmcp::deadline::Deadline;
use ripmcp::error::{Error, ErrorKind};
use ripmcp::mcp::{CancellationToken, Operation};
use ripmcp::storage::Paths;
use ripmcp::supervisor::{Action, Connection, LocalRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

fn operation(millis: u64) -> Operation {
    Operation::new(
        Deadline::new(Duration::from_millis(millis)),
        CancellationToken::new(),
    )
}

fn request(fixture: &Fixture, action: Action) -> LocalRequest {
    LocalRequest {
        cwd: fixture.root.path().to_owned(),
        server: "s".to_owned(),
        environment: BTreeMap::new(),
        action,
    }
}

async fn connection(fixture: &Fixture) -> Connection {
    Connection::ensure(
        &fixture.paths,
        Path::new(env!("CARGO_BIN_EXE_ripmcp")),
        &operation(5000),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn ipc_rechecks_policy_instead_of_accepting_a_stale_client_snapshot() {
    let fixture: Fixture = Fixture::new();
    let mut config: Value = fixture.configure("echo");
    let connection: Connection = connection(&fixture).await;
    let request: LocalRequest = request(&fixture, Action::Start);
    config["servers"]["s"]["enabled"] = json!(false);
    fixture.write_user(&config);
    let error: Error = connection
        .local(request, &operation(1000))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Configuration);
    assert!(!fixture.root.path().join("server/pid").exists());
}

#[tokio::test]
async fn per_server_queue_is_bounded_and_disconnect_releases_queued_work() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("cancel_ack");
    connection(&fixture)
        .await
        .local(request(&fixture, Action::Start), &operation(5000))
        .await
        .unwrap();
    let cancellation: CancellationToken = CancellationToken::new();
    let mut tasks: tokio::task::JoinSet<Result<Value, Error>> = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let paths: Paths = Paths::new(fixture.env.clone());
        let request: LocalRequest = request(
            &fixture,
            Action::Call {
                tool: "echo".to_owned(),
                arguments: "{\"stall\":true}".to_owned(),
            },
        );
        let token: CancellationToken = cancellation.clone();
        tasks.spawn(async move {
            let operation: Operation = Operation::new(Deadline::new(Duration::from_secs(5)), token);
            Connection::existing(&paths, &operation)
                .await?
                .unwrap()
                .local(request, &operation)
                .await
        });
    }
    let mut full = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let result: Result<Value, Error> = connection(&fixture)
            .await
            .local(request(&fixture, Action::Start), &operation(20))
            .await;
        if result.is_err_and(|error| error.message.contains("queue is full")) {
            full = true;
            break;
        }
    }
    cancellation.cancel();
    while tasks.join_next().await.is_some() {}
    assert!(full);
    let value: Value = connection(&fixture)
        .await
        .local(
            request(
                &fixture,
                Action::Call {
                    tool: "echo".to_owned(),
                    arguments: "{}".to_owned(),
                },
            ),
            &operation(5000),
        )
        .await
        .unwrap();
    assert_eq!(value["structuredContent"]["pid"], fixture.pid());
    assert_eq!(fixture.launches(), 1);
}

#[tokio::test]
async fn shutdown_cancels_active_requests_before_draining_owned_children() {
    let fixture: Fixture = Fixture::new();
    fixture.configure("echo");
    let paths: Paths = Paths::new(fixture.env.clone());
    let call: LocalRequest = request(
        &fixture,
        Action::Call {
            tool: "echo".to_owned(),
            arguments: "{\"stall\":true}".to_owned(),
        },
    );
    let connection: Connection = connection(&fixture).await;
    let task: tokio::task::JoinHandle<Result<Value, Error>> =
        tokio::spawn(async move { connection.local(call, &operation(5000)).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !fixture.root.path().join("server/calls").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    Connection::existing(&paths, &operation(1000))
        .await
        .unwrap()
        .unwrap()
        .shutdown(&operation(1000))
        .await
        .unwrap();
    assert!(task.await.unwrap().is_err());
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.runtime().join("supervisor.sock").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{}", fixture.pid())).exists());
}
