use super::super::{connection_error, protocol, protocol_error};
use crate::error::Error;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{OwnedSemaphorePermit, mpsc, oneshot};

pub(super) struct WriteMessage {
    pub bytes: Vec<u8>,
    pub permit: Option<OwnedSemaphorePermit>,
}

struct Entry {
    sender: oneshot::Sender<Result<Value, Error>>,
    permit: OwnedSemaphorePermit,
}

#[derive(Default)]
struct State {
    pending: HashMap<String, Entry>,
    closed: Option<Error>,
    last_id: u64,
}

#[derive(Default)]
pub(super) struct Registry {
    state: Mutex<State>,
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn register(
        self: &Arc<Self>,
        id: String,
        sender: oneshot::Sender<Result<Value, Error>>,
        permit: OwnedSemaphorePermit,
        writer: mpsc::Sender<WriteMessage>,
    ) -> Result<PendingRequest, Error> {
        let mut state: MutexGuard<'_, State> = self.lock();
        if let Some(error) = &state.closed {
            return Err(error.clone());
        }
        state.last_id = state.last_id.max(number(&id)?);
        state.pending.insert(id.clone(), Entry { sender, permit });
        Ok(PendingRequest {
            id,
            registry: self.clone(),
            writer,
            submitted: false,
        })
    }

    pub(super) fn receive(&self, bytes: &[u8]) -> Result<(), Error> {
        let protocol::Message::Response { id, result }: protocol::Message =
            protocol::decode(bytes)?
        else {
            return Ok(());
        };
        let id: String = id.ok_or_else(protocol_error)?;
        let mut state: MutexGuard<'_, State> = self.lock();
        if let Some(entry) = state.pending.remove(&id) {
            let _: Result<(), Result<Value, Error>> = entry.sender.send(result);
        } else if number(&id)? > state.last_id {
            return Err(protocol_error());
        }
        Ok(())
    }

    pub(super) fn close(&self, error: Error) {
        let mut state: MutexGuard<'_, State> = self.lock();
        if state.closed.is_none() {
            state.closed = Some(error.clone());
        }
        for (_, entry) in state.pending.drain() {
            let _: Result<(), Result<Value, Error>> = entry.sender.send(Err(error.clone()));
        }
    }
}

fn number(id: &str) -> Result<u64, Error> {
    id.strip_prefix("ripmcp-")
        .and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(protocol_error)
}

pub(super) struct PendingRequest {
    id: String,
    registry: Arc<Registry>,
    writer: mpsc::Sender<WriteMessage>,
    pub submitted: bool,
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        let entry: Option<Entry> = self.registry.lock().pending.remove(&self.id);
        if !self.submitted {
            return;
        }
        if let Some(entry) = entry {
            let notification: Value = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": self.id}});
            let bytes: Vec<u8> = serde_json::to_vec(&notification).unwrap_or_default();
            // Keep the request slot until cancellation is written, bounding queued frames.
            if self
                .writer
                .try_send(WriteMessage {
                    bytes,
                    permit: Some(entry.permit),
                })
                .is_err()
            {
                self.registry.close(connection_error());
            }
        }
    }
}
