mod session;

use super::endpoint::{Endpoint, Record};
use super::manager::Manager;
use super::{Barrier, invalid, unavailable};
use crate::error::Error;
use crate::storage::{Directory, Location, Paths};
use std::fs::File;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub(super) async fn serve() -> Result<(), Error> {
    let paths: Paths = Paths::from_environment();
    let endpoint: Endpoint = Endpoint::open(&paths, true)?.ok_or_else(invalid)?;
    // State-scoped locks also fence clients that change XDG_RUNTIME_DIR.
    let state: Directory =
        Directory::open(&paths.directory(Location::State)?, true, true)?.ok_or_else(invalid)?;
    let lock: File = state
        .file(
            ".supervisor.lock",
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CREATE,
            true,
        )?
        .ok_or_else(invalid)?;
    lock.try_lock().map_err(|_| unavailable())?;
    endpoint.reconcile().await?;
    let mut terminate: tokio::signal::unix::Signal =
        signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt: tokio::signal::unix::Signal =
        signal(tokio::signal::unix::SignalKind::interrupt())?;
    let (listener, record): (UnixListener, Record) = endpoint.bind()?;
    let record: Arc<Record> = Arc::new(record);
    let stop: CancellationToken = CancellationToken::new();
    let manager: Arc<Manager> = Arc::new(Manager::new(paths, Barrier::default()));
    let session: Arc<session::Session> = Arc::new(session::Session {
        record: record.clone(),
        stop: stop.clone(),
        manager: manager.clone(),
    });
    let result: Result<(), Error> = tokio::select! {
        result = accept(&listener, session) => result,
        _ = terminate.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    };
    stop.cancel();
    manager.shutdown().await;
    endpoint.remove(&record)?;
    result
}

fn signal(kind: tokio::signal::unix::SignalKind) -> Result<tokio::signal::unix::Signal, Error> {
    tokio::signal::unix::signal(kind).map_err(crate::storage::io_error)
}

async fn accept(listener: &UnixListener, session: Arc<session::Session>) -> Result<(), Error> {
    let permits: Arc<Semaphore> = Arc::new(Semaphore::new(32));
    let mut tasks: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = session.stop.cancelled() => break,
            _ = tasks.join_next(), if !tasks.is_empty() => (),
            accepted = listener.accept() => {
                let (stream, _): (UnixStream, tokio::net::unix::SocketAddr) = accepted.map_err(|_| unavailable())?;
                if let Ok(permit) = permits.clone().try_acquire_owned() {
                    let session: Arc<session::Session> = session.clone();
                    tasks.spawn(async move { session.run(stream, permit).await; });
                }
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}
