mod execute;
mod install;
pub(crate) mod status;

use super::Barrier;
use super::launch::Prepared;
use crate::config::{AuthorizedServer, Effective, Identity};
use crate::error::{Error, ErrorKind};
use crate::mcp::{Client, Operation};
use crate::storage::Paths;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, MutexGuard, OwnedRwLockReadGuard, OwnedSemaphorePermit, Semaphore};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRequest {
    pub cwd: PathBuf,
    pub server: String,
    pub environment: BTreeMap<String, String>,
    pub action: Action,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Install(Box<crate::install::Request>),
    Start,
    Stop,
    Tools { all: bool },
    Tool { tool: String },
    Call { tool: String, arguments: String },
}

pub(super) struct Manager {
    paths: Paths,
    slots: Mutex<BTreeMap<String, Arc<Slot>>>,
    pub barrier: Barrier,
}

struct Slot {
    state: Mutex<State>,
    queue: Arc<Semaphore>,
}

#[derive(Default)]
struct State {
    client: Option<Client>,
    revision: String,
    configuration_digest: String,
    installation: Option<crate::ownership::InstallationId>,
    observed: Option<Instant>,
    failed: bool,
}

impl Manager {
    pub fn new(paths: Paths, barrier: Barrier) -> Self {
        Self {
            paths,
            slots: Mutex::new(BTreeMap::new()),
            barrier,
        }
    }

    pub async fn local(
        &self,
        request: LocalRequest,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let _barrier: OwnedRwLockReadGuard<()> = operation.run(self.barrier.enter()).await?;
        if matches!(request.action, Action::Install(_)) {
            return self.install(request, operation).await;
        }
        let effective: Effective = self.effective(&request)?;
        let key: String = key(effective.identity(&request.server)?)?;
        let slot: Arc<Slot> = operation.run(self.slot(&key)).await?;
        let _queue: OwnedSemaphorePermit =
            slot.queue.clone().try_acquire_owned().map_err(|_| busy())?;
        let mut state: MutexGuard<'_, State> =
            operation.run(async { Ok(slot.state.lock().await) }).await?;
        let effective: Effective = self.effective(&request)?;
        if key != self::key(effective.identity(&request.server)?)? {
            return Err(changed());
        }
        if matches!(request.action, Action::Stop) {
            stop(&mut state).await;
            self.require_clean(&key)?;
            return Ok(action(&request.server, "stop", false));
        }
        let server: AuthorizedServer<'_> = effective.authorize(&request.server)?;
        let prepared: Prepared =
            Prepared::load(&server, &self.paths, &request.environment, &key, operation).await?;
        let current: Effective = self.effective(&request)?;
        if current.identity(&request.server)? != server.identity() {
            return Err(changed());
        }
        self.execute(&mut state, prepared, &request, &key, operation)
            .await
    }

    fn effective(&self, request: &LocalRequest) -> Result<Effective, Error> {
        let effective: Effective = Effective::load(&self.paths, &request.cwd)?;
        effective.identity(&request.server)?;
        if !effective.is_local(&request.server) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "remote servers have no local lifecycle",
            ));
        }
        if !matches!(request.action, Action::Stop) {
            let server: AuthorizedServer<'_> = effective.authorize(&request.server)?;
            let tool: Option<&str> = match &request.action {
                Action::Call { tool, .. } | Action::Tool { tool } => Some(tool),
                _ => None,
            };
            server.require_enabled(tool)?;
        }
        Ok(effective)
    }

    async fn slot(&self, key: &str) -> Result<Arc<Slot>, Error> {
        let mut slots: MutexGuard<'_, BTreeMap<String, Arc<Slot>>> = self.slots.lock().await;
        if !slots.contains_key(key) && slots.len() >= 256 {
            return Err(busy());
        }
        Ok(slots
            .entry(key.to_owned())
            .or_insert_with(|| {
                Arc::new(Slot {
                    state: Mutex::new(State::default()),
                    queue: Arc::new(Semaphore::new(16)),
                })
            })
            .clone())
    }

    pub async fn shutdown(&self) {
        let _barrier: tokio::sync::OwnedRwLockWriteGuard<()> = self.barrier.close().await;
        let slots: MutexGuard<'_, BTreeMap<String, Arc<Slot>>> = self.slots.lock().await;
        let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        for slot in slots.values() {
            let slot: Arc<Slot> = slot.clone();
            tasks.spawn(async move {
                stop(&mut *slot.state.lock().await).await;
            });
        }
        while tasks.join_next().await.is_some() {}
    }

    async fn lease(&self, key: &str, operation: &Operation) -> Result<std::fs::File, Error> {
        let directory: crate::storage::Directory = crate::storage::Directory::open(
            &self.paths.directory(crate::storage::Location::State)?,
            true,
            true,
        )?
        .ok_or_else(super::invalid)?;
        let file: std::fs::File = directory
            .file(
                &format!(".instance-{key}.lock"),
                rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CREATE,
                true,
            )?
            .ok_or_else(super::invalid)?;
        operation.run(super::client::acquire(&file)).await?;
        if directory
            .read(&format!(".instance-{key}.failed"), true)?
            .is_some()
        {
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "owned resource cleanup requires recovery",
            ));
        }
        Ok(file)
    }

    fn require_clean(&self, key: &str) -> Result<(), Error> {
        if let Some(directory) = crate::storage::Directory::open(
            &self.paths.directory(crate::storage::Location::State)?,
            false,
            true,
        )? && directory
            .read(&format!(".instance-{key}.failed"), true)?
            .is_some()
        {
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "owned resource cleanup failed; recovery record retained",
            ));
        }
        if let Some(directory) = crate::storage::Directory::open(
            &self.paths.directory(crate::storage::Location::State)?,
            false,
            true,
        )? && let Some(lock) = directory.file(
            &format!(".instance-{key}.lock"),
            rustix::fs::OFlags::RDWR,
            true,
        )? && lock.try_lock().is_err()
        {
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "owned process lease is still held; cleanup is incomplete",
            ));
        }
        Ok(())
    }
}

async fn stop(state: &mut State) {
    if let Some(client) = &state.client {
        client.shutdown().await;
    }
    state.client = None;
    state.installation = None;
    state.observed = None;
    state.failed = false;
}

fn key(identity: &Identity) -> Result<String, Error> {
    serde_json::to_vec(&(&identity.source, &identity.name))
        .map(|bytes| crate::config::digest(&bytes))
        .map_err(crate::storage::io_error)
}

fn action(server: &str, verb: &str, reused: bool) -> Value {
    json!({"schema_version": 1, "actions": [{"server": server, "action": verb, "reused": reused}]})
}

fn changed() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "server configuration changed during the operation",
    )
}
fn busy() -> Error {
    Error::new(ErrorKind::Connection, "supervisor request queue is full")
}

#[cfg(test)]
mod tests {
    use super::{Action, LocalRequest, Manager};
    use crate::deadline::Deadline;
    use crate::error::{Error, ErrorKind};
    use crate::mcp::{CancellationToken, Operation};
    use crate::storage::Paths;
    use crate::supervisor::Barrier;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::Duration;

    #[tokio::test]
    async fn maintenance_blocks_local_mutations_before_slot_allocation() {
        let root: tempfile::TempDir = tempfile::tempdir().unwrap();
        let paths: Paths = Paths::new(BTreeMap::from([(
            OsString::from("HOME"),
            root.path().as_os_str().to_owned(),
        )]));
        let manager: Manager = Manager::new(paths, Barrier::default());
        let _maintenance: tokio::sync::OwnedRwLockWriteGuard<()> =
            manager.barrier.maintenance().await.unwrap();
        let operation: Operation = Operation::new(
            Deadline::new(Duration::from_millis(10)),
            CancellationToken::new(),
        );
        let request: LocalRequest = LocalRequest {
            cwd: root.path().to_owned(),
            server: "s".to_owned(),
            environment: BTreeMap::new(),
            action: Action::Start,
        };
        let error: Error = manager.local(request, &operation).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert!(manager.slots.lock().await.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
