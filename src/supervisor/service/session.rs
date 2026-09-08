use super::super::endpoint::{Endpoint, Record};
use super::super::manager::Manager;
use super::super::wire::{self, Hello, Reply, Request, Response};
use super::super::{LocalRequest, incompatible, unavailable};
use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use crate::mcp::Operation;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::sync::{OwnedRwLockReadGuard, OwnedSemaphorePermit};
use tokio_util::sync::CancellationToken;

pub(super) struct Session {
    pub record: Arc<Record>,
    pub stop: CancellationToken,
    pub manager: Arc<Manager>,
}

impl Session {
    pub async fn run(&self, mut stream: UnixStream, _permit: OwnedSemaphorePermit) {
        let _: Result<(), Error> = tokio::select! {
            () = self.stop.cancelled() => Ok(()),
            result = async {
                let request: Request = tokio::time::timeout(Duration::from_secs(60), self.initialize(&mut stream))
                    .await.map_err(|_| unavailable())??;
                self.request(&mut stream, request).await
            } => result,
        };
    }

    async fn initialize(&self, stream: &mut UnixStream) -> Result<Request, Error> {
        Endpoint::authenticate(stream, None)?;
        let hello: Hello = wire::read(stream).await?;
        if hello.protocol != wire::VERSION
            || hello.build != self.record.build
            || hello.nonce != self.record.nonce.as_str()
            || hello.context != self.record.context
        {
            wire::write(stream, &Reply::Incompatible).await?;
            return Err(incompatible());
        }
        wire::write(stream, &Reply::Ready).await?;
        wire::read_limit(stream, wire::DATA_LIMIT).await
    }

    async fn request(&self, stream: &mut UnixStream, request: Request) -> Result<(), Error> {
        match request {
            Request::Ping => {
                let _guard: OwnedRwLockReadGuard<()> = self.manager.barrier.enter().await?;
                wire::write(stream, &Reply::Pong).await
            }
            Request::Shutdown => {
                wire::write(stream, &Reply::Stopping).await?;
                self.stop.cancel();
                Ok(())
            }
            Request::Status { cwd } => tokio::time::timeout(
                Duration::from_secs(60),
                respond(stream, self.manager.status(&cwd).await),
            )
            .await
            .map_err(|_| unavailable())?,
            Request::Local {
                request,
                budget,
                sent,
            } => self.local(stream, *request, budget, sent).await,
        }
    }

    async fn local(
        &self,
        stream: &mut UnixStream,
        request: LocalRequest,
        budget: Duration,
        sent: Duration,
    ) -> Result<(), Error> {
        let Some(elapsed): Option<Duration> = wire::now().checked_sub(sent) else {
            return Err(incompatible());
        };
        let remaining: Duration = budget.checked_sub(elapsed).unwrap_or_default();
        let cancellation: CancellationToken = self.stop.child_token();
        let _cancel: tokio_util::sync::DropGuard = cancellation.clone().drop_guard();
        let operation: Arc<Operation> = Arc::new(Operation::new(
            Deadline::new(remaining),
            cancellation.clone(),
        ));
        let task_operation: Arc<Operation> = operation.clone();
        let manager: Arc<Manager> = self.manager.clone();
        let mut task: tokio::task::JoinHandle<Result<Value, Error>> =
            tokio::spawn(async move { manager.local(request, &task_operation).await });
        let mut byte: [u8; 1] = [0];
        let result: Result<Value, Error> = tokio::select! {
            biased;
            _ = stream.read(&mut byte) => {
                cancellation.cancel();
                let _: Result<Result<Value, Error>, tokio::task::JoinError> = task.await;
                return Err(unavailable());
            },
            result = &mut task => result.map_err(|_| unavailable())?,
        };
        if operation.remaining().is_err() {
            return respond(
                stream,
                Err(Error::new(
                    ErrorKind::Timeout,
                    "operation timed out; completion may be unknown",
                )),
            )
            .await;
        }
        operation.run(respond(stream, result)).await
    }
}

async fn respond(stream: &mut UnixStream, result: Result<Value, Error>) -> Result<(), Error> {
    let response: Response = match result {
        Ok(value) => Response::Success(serde_json::to_string(&value).map_err(|_| incompatible())?),
        Err(error) => Response::Failure {
            kind: error.kind,
            message: error.message.into_owned(),
        },
    };
    wire::write_limit(stream, &response, wire::DATA_LIMIT).await
}
