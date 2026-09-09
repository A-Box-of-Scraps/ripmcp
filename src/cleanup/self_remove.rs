use super::{
    Report,
    self_plan::{Request, SelfPlan},
};
use crate::{
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
    ownership::Journal,
    storage::{Location, Maintenance, Paths, Store},
    supervisor::{Action, Connection, LocalRequest},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Serialize)]
pub(crate) struct SelfReport {
    pub schema_version: u32,
    pub plan: SelfPlan,
    pub completed: Vec<Value>,
    pub failures: Vec<Value>,
    pub installations: Vec<Report>,
    pub configuration_after: Option<String>,
}

impl SelfReport {
    pub fn failure(&mut self, action: &str, error: &Error) {
        self.failures
            .push(json!({"action": action, "cause": error.message}));
    }
}

pub fn run(yes: bool, timeout: Option<u64>) -> Result<(), Error> {
    super::require_terminal(true, yes)?;
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(execute(yes, timeout))
}

async fn execute(yes: bool, timeout: Option<u64>) -> Result<(), Error> {
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let executable: PathBuf = std::env::current_exe().map_err(crate::storage::io_error)?;
    let plan: SelfPlan = SelfPlan::build(&paths, &cwd, &executable)?;
    super::confirm(&plan, yes)?;
    let signal: SignalCancellation = SignalCancellation::install()?;
    let operation: Operation = Operation::new(
        crate::deadline::Deadline::new(Duration::from_secs(timeout.unwrap_or(60))),
        signal.token(),
    );
    let maintenance: Maintenance = Maintenance::acquire(&paths, true, &operation).await?;
    let approval: String = plan.digest()?;
    if SelfPlan::build(&paths, &cwd, &executable)?.digest()? != approval {
        return Err(super::changed());
    }
    let marker: Store = Store::new(paths.directory(Location::State)?, "self-removal.json", true)?;
    super::self_state::save(
        &paths,
        &marker,
        &json!({"schema_version": 1, "state": "applying", "plan": &plan}),
        &operation,
    )
    .await?;
    super::self_state::restore(&paths, &operation).await?;
    let request: LocalRequest = LocalRequest {
        cwd,
        server: String::new(),
        environment: BTreeMap::new(),
        action: Action::SelfUninstall(Request {
            approval,
            executable,
        }),
    };
    let value: Value = match Connection::existing(&paths, &operation).await? {
        Some(connection) => connection.local(request, &operation).await?,
        None => crate::supervisor::offline_cleanup(paths.clone(), request, &operation).await?,
    };
    let mut value: Value = finalize(&paths, &plan, value, &operation).await;
    super::self_state::save(&paths, &marker, &value, &operation).await?;
    if value["failures"].as_array().is_some_and(Vec::is_empty)
        && let Err(error) = finish_resources(&paths, &plan)
    {
        append_failure(&mut value, "remove_executable", &error);
    }
    if value["failures"].as_array().is_some_and(Vec::is_empty) {
        maintenance.finish()?;
        if let Some(completed) = value["completed"].as_array_mut() {
            completed
                .push(json!({"action": "remove_standalone_executable", "path": plan.executable}));
        }
    } else {
        super::self_state::save(&paths, &marker, &value, &operation).await?;
    }
    crate::output::json(&mut std::io::stdout().lock(), &value)?;
    if value["failures"]
        .as_array()
        .is_some_and(|failures| !failures.is_empty())
    {
        return Err(Error::new(
            ErrorKind::PartialFailure,
            "self-removal incomplete; retry --uninstall-everything -y; failure records and executable provenance retained",
        ));
    }
    Ok(())
}

async fn finalize(
    paths: &Paths,
    plan: &SelfPlan,
    mut value: Value,
    operation: &Operation,
) -> Value {
    if let Err(error) = stop_supervisor(paths, operation).await {
        append_failure(&mut value, "stop_supervisor", &error);
    }
    if value["failures"].as_array().is_some_and(Vec::is_empty)
        && let Err(error) = super::self_work::unregister(paths, plan, &mut value, operation).await
    {
        append_failure(&mut value, "unregister_user_scope", &error);
    }
    if value["failures"].as_array().is_some_and(Vec::is_empty)
        && let Err(error) =
            super::self_work::installations(paths, plan, &mut value, operation).await
    {
        append_failure(&mut value, "cleanup_installations", &error);
    }
    for key in &plan.credentials {
        let result: Result<(), Error> =
            crate::auth::cleanup::remove(&crate::auth::SystemStore, key, operation).await;
        if let Err(error) = result {
            append_failure(&mut value, "remove_oauth_credentials", &error);
        }
    }
    if value["failures"].as_array().is_some_and(Vec::is_empty)
        && let Err(error) = super::self_artifacts::clean(paths, plan, &mut value, operation).await
    {
        append_failure(&mut value, "delete_application_artifacts", &error);
    }
    if value["failures"].as_array().is_some_and(Vec::is_empty)
        && let Err(error) = super::self_work::setup(paths, plan, &mut value, operation).await
    {
        append_failure(&mut value, "remove_installation_links", &error);
    }
    value
}

async fn stop_supervisor(paths: &Paths, operation: &Operation) -> Result<(), Error> {
    if let Some(connection) = Connection::existing(paths, operation).await? {
        connection.shutdown(operation).await?;
    }
    let directory: crate::storage::Directory =
        crate::storage::Directory::open(&paths.directory(Location::State)?, false, true)?
            .ok_or_else(super::changed)?;
    if let Some(file) = directory.file(".supervisor.lock", rustix::fs::OFlags::RDWR, true)? {
        operation
            .run(crate::storage::acquire_file(&file, true))
            .await?;
    }
    Ok(())
}

fn finish_resources(paths: &Paths, plan: &SelfPlan) -> Result<(), Error> {
    let journal: Journal = super::self_state::ownership(paths)?;
    let crate::ownership::Executable::Standalone {
        resource, sha256, ..
    }: &crate::ownership::Executable = &journal.executable
    else {
        return Err(Error {
            kind: ErrorKind::PartialFailure,
            message: plan.executable_action.clone().into(),
        });
    };
    if resource.identity
        != (crate::ownership::ResourceIdentity::Path {
            canonical_path: plan.executable.clone(),
        })
        || crate::ownership::executable::digest(&plan.executable)? != *sha256
    {
        return Err(super::changed());
    }
    if journal
        .installations
        .values()
        .flat_map(|entry| &entry.resources)
        .any(|other| other.identity == resource.identity)
    {
        return Err(super::changed());
    }
    super::self_artifacts::metadata(paths)?;
    super::remove::remove(resource)
}

pub(crate) async fn save(store: &Store, value: &Value, operation: &Operation) -> Result<(), Error> {
    let locked: crate::storage::LockedStore = store.lock(operation).await?;
    locked.commit(
        &serde_json::to_vec_pretty(value).map_err(crate::storage::io_error)?,
        operation,
    )
}

pub(crate) fn append_failure(value: &mut Value, action: &str, error: &Error) {
    if let Some(failures) = value["failures"].as_array_mut() {
        failures.push(json!({"action": action, "cause": error.message}));
    }
}
