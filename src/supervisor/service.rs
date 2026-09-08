use super::endpoint::{Endpoint, Record};
use super::wire::{self, Hello, Reply, Request};
use super::{invalid, unavailable};
use crate::error::Error;
use crate::storage::Paths;
use std::fs::File;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{
    OwnedRwLockReadGuard, OwnedRwLockWriteGuard, OwnedSemaphorePermit, RwLock, Semaphore,
};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct Barrier {
    gate: Arc<RwLock<()>>,
    closing: Arc<AtomicBool>,
}

impl Barrier {
    pub async fn enter(&self) -> Result<OwnedRwLockReadGuard<()>, Error> {
        let guard: OwnedRwLockReadGuard<()> = self.gate.clone().read_owned().await;
        self.check()?;
        Ok(guard)
    }

    pub async fn maintenance(&self) -> Result<OwnedRwLockWriteGuard<()>, Error> {
        let guard: OwnedRwLockWriteGuard<()> = self.gate.clone().write_owned().await;
        self.check()?;
        Ok(guard)
    }

    pub async fn close(&self) -> OwnedRwLockWriteGuard<()> {
        self.closing.store(true, Ordering::Release);
        self.gate.clone().write_owned().await
    }

    fn check(&self) -> Result<(), Error> {
        if self.closing.load(Ordering::Acquire) {
            Err(unavailable())
        } else {
            Ok(())
        }
    }
}

pub(super) async fn serve() -> Result<(), Error> {
    let paths: Paths = Paths::from_environment();
    let endpoint: Endpoint = Endpoint::open(&paths, true)?.ok_or_else(invalid)?;
    let lock: File = endpoint.lock(".supervisor.lock")?;
    lock.try_lock().map_err(|_| unavailable())?;
    endpoint.reconcile().await?;
    let mut terminate: tokio::signal::unix::Signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(crate::storage::io_error)?;
    let mut interrupt: tokio::signal::unix::Signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map_err(crate::storage::io_error)?;
    let (listener, record): (UnixListener, Record) = endpoint.bind()?;
    let record: Arc<Record> = Arc::new(record);
    let stop: CancellationToken = CancellationToken::new();
    let barrier: Barrier = Barrier::default();
    let result: Result<(), Error> = tokio::select! {
        result = accept(&listener, record.clone(), stop.clone(), barrier.clone()) => result,
        _ = terminate.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    };
    stop.cancel();
    let _guard: OwnedRwLockWriteGuard<()> = barrier.close().await;
    endpoint.remove(&record)?;
    result
}

async fn accept(
    listener: &UnixListener,
    record: Arc<Record>,
    stop: CancellationToken,
    barrier: Barrier,
) -> Result<(), Error> {
    let permits: Arc<Semaphore> = Arc::new(Semaphore::new(32));
    let mut tasks: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            _ = tasks.join_next(), if !tasks.is_empty() => (),
            accepted = listener.accept() => {
                let (stream, _): (UnixStream, tokio::net::unix::SocketAddr) =
                    accepted.map_err(|_| unavailable())?;
                if let Ok(permit) = permits.clone().try_acquire_owned() {
                    tasks.spawn(session(stream, record.clone(), stop.clone(), barrier.clone(), permit));
                }
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

async fn session(
    mut stream: UnixStream,
    record: Arc<Record>,
    stop: CancellationToken,
    barrier: Barrier,
    _permit: OwnedSemaphorePermit,
) {
    let _: Result<(), Error> = tokio::select! {
        () = stop.cancelled() => Ok(()),
        result = tokio::time::timeout(Duration::from_secs(60), async {
            Endpoint::authenticate(&stream, None)?;
            handshake(&mut stream, &record).await?;
            request(&mut stream, &barrier, &stop).await
        }) => result.unwrap_or_else(|_| Err(unavailable())),
    };
}

async fn handshake(stream: &mut UnixStream, record: &Record) -> Result<(), Error> {
    let hello: Hello = wire::read(stream).await?;
    if hello.protocol != wire::VERSION
        || hello.build != record.build
        || hello.nonce != record.nonce.as_str()
        || hello.context != record.context
    {
        wire::write(stream, &Reply::Incompatible).await?;
        return Err(super::incompatible());
    }
    wire::write(stream, &Reply::Ready).await
}

async fn request(
    stream: &mut UnixStream,
    barrier: &Barrier,
    stop: &CancellationToken,
) -> Result<(), Error> {
    match wire::read::<Request>(stream).await? {
        Request::Ping => {
            let _guard: OwnedRwLockReadGuard<()> = barrier.enter().await?;
            wire::write(stream, &Reply::Pong).await
        }
        Request::Shutdown => {
            let _guard: OwnedRwLockWriteGuard<()> = barrier.close().await;
            wire::write(stream, &Reply::Stopping).await?;
            stop.cancel();
            Ok(())
        }
    }
}
