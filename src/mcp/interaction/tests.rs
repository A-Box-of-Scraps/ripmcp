use super::{Pending, Prompt, continue_request};
use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use crate::mcp::{CancellationToken, Operation};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;

fn operation() -> Operation {
    Operation::new(
        Deadline::new(Duration::from_secs(2)),
        CancellationToken::new(),
    )
}

fn input() -> Value {
    json!({"resultType": "input_required", "requestState": "opaque",
    "inputRequests": {"login": {"method": "elicitation/create", "params": {
        "mode": "url", "url": "https://example.com/authorize", "message": "Authorize"
    }}}})
}

#[test]
fn urls_reject_unsafe_schemes_credentials_and_control_characters() {
    for url in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://example.com/",
        "https://user:secret@example.com/",
        "https://example.com/\nsecret",
        "https://example.com/\u{202e}secret",
        "https:///",
    ] {
        let prompt: Prompt = Prompt {
            url: url.to_owned(),
            message: "Login".to_owned(),
        };
        assert!(prompt.validate().is_err(), "{url:?}");
    }
    for url in [
        "https://example.com/login",
        "http://127.0.0.1:8085/",
        "http://[::1]:8085/",
        "http://localhost:8085/",
    ] {
        let prompt: Prompt = Prompt {
            url: url.to_owned(),
            message: "Login".to_owned(),
        };
        assert!(prompt.validate().is_ok(), "{url:?}");
    }
}

#[tokio::test]
async fn continuation_waits_for_presentation_and_preserves_opaque_state() {
    let (sender, mut receiver): (mpsc::Sender<Pending>, mpsc::Receiver<Pending>) = mpsc::channel(1);
    let mut operation: Operation = operation();
    operation.forward_interaction(sender);
    let task: tokio::task::JoinHandle<Value> = tokio::spawn(async move {
        let mut params: Value = json!({"name": "write", "arguments": {"value": 1}});
        continue_request(&input(), &mut params, &operation)
            .await
            .unwrap();
        params
    });
    let pending: Pending = receiver.recv().await.unwrap();
    assert_eq!(pending.prompt.message, "Authorize");
    assert!(!task.is_finished());
    pending.presented.send(()).unwrap();
    let params: Value = task.await.unwrap();
    assert_eq!(
        params,
        json!({"name": "write", "arguments": {"value": 1},
        "inputResponses": {"login": {"action": "accept"}}, "requestState": "opaque"})
    );
}

#[tokio::test]
async fn validates_entire_input_batch_before_presenting_any_prompt() {
    for mode in ["form", "sampling", "invalid_url", "state", "meta", "empty"] {
        let (sender, mut receiver): (mpsc::Sender<Pending>, mpsc::Receiver<Pending>) =
            mpsc::channel(1);
        let mut operation: Operation = operation();
        operation.forward_interaction(sender);
        let mut result: Value = input();
        result["inputRequests"]["z"] = result["inputRequests"]["login"].clone();
        match mode {
            "form" => result["inputRequests"]["z"]["params"]["mode"] = json!("form"),
            "sampling" => result["inputRequests"]["z"]["method"] = json!("sampling/createMessage"),
            "invalid_url" => {
                result["inputRequests"]["z"]["params"]["url"] = json!("file:///secret")
            }
            "state" => result["requestState"] = json!(42),
            "meta" => result["_meta"] = json!(42),
            _ => result["inputRequests"] = json!({}),
        }
        let mut params: Value = json!({});
        assert!(
            continue_request(&result, &mut params, &operation)
                .await
                .is_err()
        );
        assert!(receiver.try_recv().is_err());
        assert_eq!(params, json!({}));
    }
}

#[tokio::test]
async fn noninteractive_and_cancelled_operations_do_not_continue() {
    let mut params: Value = json!({});
    let error: Error = continue_request(&input(), &mut params, &operation())
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unsupported);
    let token: CancellationToken = CancellationToken::new();
    token.cancel();
    let mut operation: Operation = Operation::new(Deadline::new(Duration::from_secs(1)), token);
    operation.enable_interaction("fixture".to_owned());
    let error: Error = continue_request(&input(), &mut params, &operation)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert_eq!(params, json!({}));
}
