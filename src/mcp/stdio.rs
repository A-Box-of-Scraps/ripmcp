mod process;
mod registry;

use super::{Limits, Operation, connection_error, protocol};
use crate::error::Error;
use registry::{PendingRequest, Registry, WriteMessage};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

type Response = Result<Value, Error>;

pub struct StdioOptions {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub env: BTreeMap<OsString, OsString>,
    pub cwd: Option<PathBuf>,
}

impl StdioOptions {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct StderrLog {
    pub redacted_bytes: u64,
    pub redacted_chunks: u64,
}

pub(super) struct Stdio {
    registry: Arc<Registry>,
    writer: mpsc::Sender<WriteMessage>,
    stop: CancellationToken,
    worker: tokio::sync::Mutex<Option<JoinHandle<()>>>,
    permits: Arc<Semaphore>,
    limits: Limits,
    logs: Arc<Mutex<StderrLog>>,
}

impl Stdio {
    pub(super) fn spawn(options: StdioOptions, limits: Limits) -> Result<Self, Error> {
        let registry: Arc<Registry> = Arc::new(Registry::default());
        let (writer, receiver): (mpsc::Sender<WriteMessage>, mpsc::Receiver<WriteMessage>) =
            mpsc::channel(limits.in_flight * 2);
        let stop: CancellationToken = CancellationToken::new();
        let logs: Arc<Mutex<StderrLog>> = Arc::new(Mutex::new(StderrLog::default()));
        let worker: JoinHandle<()> = process::spawn(
            options,
            receiver,
            registry.clone(),
            stop.clone(),
            logs.clone(),
            limits.frame_bytes,
        )?;
        Ok(Self {
            registry,
            writer,
            stop,
            worker: tokio::sync::Mutex::new(Some(worker)),
            permits: Arc::new(Semaphore::new(limits.in_flight)),
            limits,
            logs,
        })
    }

    pub(super) async fn request(
        &self,
        message: &Value,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let bytes: Vec<u8> = protocol::encode(message, self.limits.frame_bytes)?;
        let id: String = message["id"]
            .as_str()
            .ok_or_else(super::protocol_error)?
            .to_owned();
        operation
            .run(async {
                let permit: tokio::sync::OwnedSemaphorePermit = self
                    .permits
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| connection_error())?;
                let (sender, receiver): (oneshot::Sender<Response>, oneshot::Receiver<Response>) =
                    oneshot::channel();
                let mut pending: PendingRequest =
                    self.registry
                        .register(id, sender, permit, self.writer.clone())?;
                self.writer
                    .send(WriteMessage {
                        bytes,
                        permit: None,
                    })
                    .await
                    .map_err(|_| connection_error())?;
                pending.submitted = true;
                receiver.await.map_err(|_| connection_error())?
            })
            .await
    }

    pub(super) fn stderr_log(&self) -> StderrLog {
        *self
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) async fn shutdown(&self) {
        self.stop.cancel();
        let mut worker: tokio::sync::MutexGuard<'_, Option<JoinHandle<()>>> =
            self.worker.lock().await;
        if let Some(worker) = worker.as_mut() {
            let _: Result<(), tokio::task::JoinError> = worker.await;
        }
        worker.take();
    }
}

impl Drop for Stdio {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
