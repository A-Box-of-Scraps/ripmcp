use super::{Action, Connection, LocalRequest};
use crate::cli::Command;
use crate::config::{AuthorizedServer, Definition, Effective, schema::SecretReference};
use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use crate::mcp::{Operation, SignalCancellation};
use crate::storage::Paths;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn run(command: Command, timeout: Option<u64>) -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(execute(command, timeout))
}

async fn execute(command: Command, timeout: Option<u64>) -> Result<(), Error> {
    let signal: SignalCancellation = SignalCancellation::install()?;
    let started: Instant = Instant::now();
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let effective: Effective = Effective::load(&paths, &cwd)?;
    let budget: Duration =
        Duration::from_secs(timeout.unwrap_or(effective.timeouts().operation_seconds.get()));
    let operation: Operation = Operation::new(
        Deadline::new(budget.saturating_sub(started.elapsed())),
        signal.token(),
    );
    if matches!(command, Command::Servers) {
        let local = effective
            .reports()
            .iter()
            .any(|report| effective.is_local(report.name));
        let connection: Option<Connection> = if local {
            Connection::existing(&paths, &operation).await?
        } else {
            None
        };
        let value: Value = match connection {
            Some(connection) => connection.status(cwd, &operation).await?,
            None => super::manager::status::offline(&paths, &cwd)?,
        };
        return crate::output::json(&mut std::io::stdout().lock(), &value);
    }
    let (name, action): (String, Action) = action(command, &effective, &operation).await?;
    if !effective.is_local(&name) {
        let value: Value = crate::auth::remote::execute(cwd, &name, &action, &operation).await?;
        return write_result(&action, &value);
    }
    let environment: BTreeMap<String, String> = if matches!(action, Action::Stop) {
        BTreeMap::new()
    } else {
        environment(&effective.authorize(&name)?)
    };
    let call = matches!(action, Action::Call { .. });
    let request: LocalRequest = LocalRequest {
        cwd,
        server: name,
        environment,
        action,
    };
    let executable: PathBuf = std::env::current_exe().map_err(crate::storage::io_error)?;
    let connection: Connection = Connection::ensure(&paths, &executable, &operation).await?;
    let value: Value = connection.local(request, &operation).await?;
    if call {
        crate::output::tool_result(&mut std::io::stdout().lock(), &value)
    } else {
        crate::output::json(&mut std::io::stdout().lock(), &value)
    }
}

fn write_result(action: &Action, value: &Value) -> Result<(), Error> {
    if matches!(action, Action::Call { .. }) {
        crate::output::tool_result(&mut std::io::stdout().lock(), value)
    } else {
        crate::output::json(&mut std::io::stdout().lock(), value)
    }
}

async fn action(
    command: Command,
    effective: &Effective,
    operation: &Operation,
) -> Result<(String, Action), Error> {
    let (name, action): (String, Action) = match command {
        Command::Start(server) => (server.server, Action::Start),
        Command::Stop(server) => (server.server, Action::Stop),
        Command::Tools(tools) => (
            tools.server.ok_or_else(unsupported)?,
            Action::Tools { all: tools.all },
        ),
        Command::Tool(tool) => (tool.server, Action::Tool { tool: tool.tool }),
        Command::Call(call) => {
            let request: crate::call::Request = call.into_request()?;
            let name: String = request.server.ok_or_else(unsupported)?;
            effective
                .authorize(&name)?
                .require_enabled(Some(&request.tool))?;
            (
                name,
                Action::Call {
                    tool: request.tool,
                    arguments: super::input::arguments(request.input, operation).await?,
                },
            )
        }
        _ => return Err(unsupported()),
    };
    effective.identity(&name)?;
    if !matches!(action, Action::Stop) {
        effective.authorize(&name)?.require_enabled(None)?;
    }
    Ok((name, action))
}

fn environment(server: &AuthorizedServer<'_>) -> BTreeMap<String, String> {
    installation_environment(server.server())
}

pub(crate) fn installation_environment(server: &crate::config::Server) -> BTreeMap<String, String> {
    let mut values: BTreeMap<String, String> = super::launch::BASE_ENV
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_owned(), value))
        })
        .collect();
    let env: &BTreeMap<String, SecretReference> = match &server.definition {
        Definition::Local { env, .. } => env,
        Definition::Remote { headers, .. } => headers,
    };
    {
        for reference in env.values() {
            if let SecretReference::Environment(name) = reference
                && let Ok(value) = std::env::var(name)
            {
                values.insert(name.clone(), value);
            }
        }
    }
    values
}

fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "cross-server tool workflow is not implemented yet",
    )
}
