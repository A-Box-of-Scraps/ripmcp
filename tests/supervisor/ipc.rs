use super::{Fixture, operation};
use ripmcp::supervisor::Connection;
use serde_json::{Value, json};
use std::fs;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub(super) fn shutdown_best_effort(fixture: &Fixture) {
    use std::io::Write;
    let Ok(mut stream): std::io::Result<std::os::unix::net::UnixStream> =
        std::os::unix::net::UnixStream::connect(fixture.runtime().join("supervisor.sock"))
    else {
        return;
    };
    let _: std::io::Result<()> = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let Ok(bytes): std::io::Result<Vec<u8>> = fs::read(fixture.runtime().join("supervisor.json"))
    else {
        return;
    };
    let Ok(record): Result<Value, serde_json::Error> = ripmcp::json::parse(&bytes) else {
        return;
    };
    let request: Value = json!({"protocol": 3, "build": env!("CARGO_PKG_VERSION"), "nonce": record["nonce"], "context": record["context"]});
    for value in [request, json!("shutdown")] {
        let bytes: Vec<u8> = serde_json::to_vec(&value).unwrap();
        let _: std::io::Result<()> = stream.write_all(&(bytes.len() as u32).to_be_bytes());
        let _: std::io::Result<()> = stream.write_all(&bytes);
    }
    for _ in 0..50 {
        if !fixture.runtime().join("supervisor.sock").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

async fn send(stream: &mut UnixStream, value: &Value) {
    let bytes: Vec<u8> = serde_json::to_vec(value).unwrap();
    stream.write_u32(bytes.len() as u32).await.unwrap();
    stream.write_all(&bytes).await.unwrap();
}

async fn receive(stream: &mut UnixStream) -> Value {
    let length = stream.read_u32().await.unwrap();
    assert!(length <= 4096);
    let mut bytes: Vec<u8> = vec![0; length as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    ripmcp::json::parse(&bytes).unwrap()
}

fn hello(fixture: &Fixture) -> Value {
    let record: Value =
        ripmcp::json::parse(&fs::read(fixture.runtime().join("supervisor.json")).unwrap()).unwrap();
    json!({"protocol": 3, "build": env!("CARGO_PKG_VERSION"), "nonce": record["nonce"], "context": record["context"]})
}

#[tokio::test]
async fn incompatible_handshakes_and_disconnected_clients_do_not_kill_the_service() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    for (field, value) in [
        ("protocol", json!(999)),
        ("build", json!("other")),
        ("nonce", json!("wrong")),
        ("context", json!("other")),
    ] {
        let mut stream: UnixStream = UnixStream::connect(fixture.runtime().join("supervisor.sock"))
            .await
            .unwrap();
        let mut request: Value = hello(&fixture);
        request[field] = value;
        send(&mut stream, &request).await;
        assert_eq!(receive(&mut stream).await, json!("incompatible"));
    }
    let mut stream: UnixStream = UnixStream::connect(fixture.runtime().join("supervisor.sock"))
        .await
        .unwrap();
    stream.write_u32(4097).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stream.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    for _ in 0..8 {
        let connection: Connection = fixture.ensure().await;
        drop(connection);
    }
    fixture.ensure().await.ping(&operation()).await.unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn changed_pid_and_socket_identity_are_rejected() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let path: std::path::PathBuf = fixture.runtime().join("supervisor.json");
    let bytes: Vec<u8> = fs::read(&path).unwrap();
    let mut record: Value = ripmcp::json::parse(&bytes).unwrap();
    record["pid"] = json!(std::process::id());
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .is_err()
    );
    record = ripmcp::json::parse(&bytes).unwrap();
    record["inode"] = json!(0);
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(
        Connection::existing(&fixture.paths, &operation())
            .await
            .is_err()
    );
    fs::write(&path, bytes).unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn a_recorded_interrupted_publication_can_be_recovered() {
    let fixture: Fixture = Fixture::new();
    let mut daemon: std::process::Child = fixture.daemon();
    tokio::time::timeout(Duration::from_secs(5), fixture.wait_ready())
        .await
        .unwrap();
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    fs::rename(
        fixture.runtime().join("supervisor.sock"),
        fixture.runtime().join(".supervisor.socket.pending"),
    )
    .unwrap();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn connection_admission_is_bounded_and_recovers_after_disconnect() {
    let fixture: Fixture = Fixture::new();
    fixture.ensure().await.ping(&operation()).await.unwrap();
    let mut streams: Vec<UnixStream> = Vec::new();
    for _ in 0..32 {
        let mut stream: UnixStream = UnixStream::connect(fixture.runtime().join("supervisor.sock"))
            .await
            .unwrap();
        send(&mut stream, &hello(&fixture)).await;
        assert_eq!(receive(&mut stream).await, json!("ready"));
        streams.push(stream);
    }
    let mut overflow: UnixStream = UnixStream::connect(fixture.runtime().join("supervisor.sock"))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), overflow.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    drop(streams);
    tokio::time::sleep(Duration::from_millis(20)).await;
    fixture.ensure().await.ping(&operation()).await.unwrap();
    fixture.stop().await;
}
