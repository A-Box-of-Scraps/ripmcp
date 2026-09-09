pub(crate) mod journal;
pub(crate) mod plan;
mod remove;
mod self_artifacts;
pub(crate) mod self_plan;
pub(crate) mod self_remove;
mod self_state;
mod self_work;
pub use self_plan::Request as SelfRequest;
pub use self_remove::run as uninstall_everything;

use crate::{
    cli::Uninstall,
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
    storage::Paths,
    supervisor::{Action, Connection, LocalRequest},
};
pub use plan::Request;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufRead, IsTerminal, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Serialize)]
pub(crate) struct Report {
    pub schema_version: u32,
    pub completed: Vec<Value>,
    pub failures: Vec<Value>,
    pub plan: plan::Plan,
}

impl Report {
    pub fn failure(
        &mut self,
        action: &str,
        resource: Option<crate::ownership::ResourceIdentity>,
        error: &Error,
    ) {
        self.failures.push(
            serde_json::json!({"action": action, "resource": resource, "cause": error.message}),
        );
    }
}

pub fn run(uninstall: Uninstall, timeout: Option<u64>) -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(execute(uninstall, timeout))
}

async fn execute(uninstall: Uninstall, timeout: Option<u64>) -> Result<(), Error> {
    require_terminal(uninstall.clean, uninstall.yes)?;
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let mut request: Request = Request {
        project: uninstall.scope.project,
        clean: uninstall.clean,
        approval: String::new(),
    };
    let plan: plan::Plan = plan::Plan::build(&paths, &cwd, &uninstall.server, &request)?;
    if uninstall.clean {
        confirm(&plan, uninstall.yes)?;
    }
    let signal: SignalCancellation = SignalCancellation::install()?;
    request.approval = plan.digest()?;
    let seconds = timeout.unwrap_or(
        crate::config::Effective::load(&paths, &cwd)?
            .timeouts()
            .operation_seconds
            .get(),
    );
    let operation: Operation = Operation::new(
        crate::deadline::Deadline::new(Duration::from_secs(seconds)),
        signal.token(),
    );
    let _maintenance: crate::storage::Maintenance =
        crate::storage::Maintenance::acquire(&paths, false, &operation).await?;
    let executable: PathBuf = std::env::current_exe().map_err(crate::storage::io_error)?;
    let connection: Connection = Connection::ensure(&paths, &executable, &operation).await?;
    let value: Value = connection
        .local(
            LocalRequest {
                cwd,
                server: uninstall.server,
                environment: BTreeMap::new(),
                action: Action::Uninstall(request),
            },
            &operation,
        )
        .await?;
    crate::output::json(&mut std::io::stdout().lock(), &value)?;
    if value["failures"]
        .as_array()
        .is_some_and(|failures| !failures.is_empty())
    {
        return Err(Error::new(
            ErrorKind::PartialFailure,
            "uninstall incomplete; retry with the exact argument vector in plan.retry from the same project directory",
        ));
    }
    Ok(())
}

pub(crate) fn require_terminal(destructive: bool, yes: bool) -> Result<(), Error> {
    if destructive && !yes && (!std::io::stdin().is_terminal() || !std::io::stderr().is_terminal())
    {
        return Err(Error::new(
            ErrorKind::Configuration,
            "cleanup requires a terminal or -y; no changes made",
        ));
    }
    Ok(())
}

pub(crate) fn confirm(plan: &impl Serialize, yes: bool) -> Result<(), Error> {
    let mut stderr: std::io::StderrLock<'static> = std::io::stderr().lock();
    crate::output::json(&mut stderr, plan)?;
    if yes {
        return Ok(());
    }
    stderr
        .write_all(b"Stop, unregister and delete exactly these owned resources? [y/N] ")
        .map_err(crate::storage::io_error)?;
    stderr.flush().map_err(crate::storage::io_error)?;
    let mut answer: String = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(crate::storage::io_error)?;
    if !matches!(answer.trim(), "y" | "Y") {
        return Err(Error::new(
            ErrorKind::Cancelled,
            "cleanup declined; no changes made",
        ));
    }
    Ok(())
}

pub(crate) fn changed() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "cleanup target, filesystem identity or ownership changed; preserved resources, preview again before retrying",
    )
}
