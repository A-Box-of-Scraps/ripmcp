mod import;
pub(crate) mod input;
pub(crate) mod preparation;
pub(crate) mod records;
pub(crate) mod report;
pub(crate) mod target;

use crate::{
    cli::Install,
    config::Effective,
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
    storage::Paths,
    supervisor::{Action, Connection, LocalRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub server: crate::config::Server,
    pub project: bool,
    pub skip_verify: bool,
    pub import_project: Option<import::Approval>,
}

pub fn run(install: Install, timeout: Option<u64>) -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(execute(install, timeout))
}

async fn execute(install: Install, timeout: Option<u64>) -> Result<(), Error> {
    let signal: SignalCancellation = SignalCancellation::install()?;
    let started: Instant = Instant::now();
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let effective: Effective = Effective::load(&paths, &cwd)?;
    target::validate_scope(&effective, &paths, install.scope.project)?;
    let budget: Duration =
        Duration::from_secs(timeout.unwrap_or(effective.timeouts().operation_seconds.get()));
    let operation: Operation = Operation::new(
        Deadline::new(budget.saturating_sub(started.elapsed())),
        signal.token(),
    );
    let import_project: Option<import::Approval> =
        import::authorize(install.source.config.as_deref(), &paths)?;
    let server: crate::config::Server = input::parse(&install)?;
    let _maintenance: crate::storage::Maintenance =
        crate::storage::Maintenance::acquire(&paths, false, &operation).await?;
    let environment: BTreeMap<String, String> =
        crate::supervisor::installation_environment(&server);
    let request: LocalRequest = LocalRequest {
        cwd,
        server: install.server,
        environment,
        action: Action::Install(Box::new(Request {
            server,
            project: install.scope.project,
            skip_verify: install.skip_verify,
            import_project,
        })),
    };
    let executable: PathBuf = std::env::current_exe().map_err(crate::storage::io_error)?;
    let connection: Connection = Connection::ensure(&paths, &executable, &operation).await?;
    let value: Value = connection.local(request, &operation).await?;
    crate::output::json(&mut std::io::stdout().lock(), &value)
}

pub(crate) fn invalid() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "invalid installation request or unresolved immutable runtime identity",
    )
}
