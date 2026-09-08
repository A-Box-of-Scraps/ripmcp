use crate::{
    cli::Policy,
    config::{Configuration, Effective, MutationReport, Server, WriteScope},
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::Operation,
    storage::Paths,
    supervisor::Action,
};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) async fn run(
    policy: &Policy,
    enabled: bool,
    paths: &Paths,
    cwd: &Path,
    effective: &Effective,
    operation: &Operation,
) -> Result<(), Error> {
    let scope: WriteScope = (&policy.scope).into();
    let target: Server = target(policy, paths, effective)?;
    if let Some(tool) = &policy.tool {
        validate_tool(policy, &target, paths, cwd, effective, operation).await?;
        if tool.is_empty() {
            return Err(unknown_tool());
        }
    }
    let deadline: Deadline = Deadline::new(operation.remaining()?);
    let ((), report): ((), MutationReport) =
        effective.mutate_with_deadline(paths, scope, &policy.server, &deadline, |config| {
            let server: &mut Server = config
                .servers
                .get_mut(&policy.server)
                .ok_or_else(unknown_server)?;
            if server != &target {
                return Err(Error::new(
                    ErrorKind::Configuration,
                    "target changed during policy validation",
                ));
            }
            match &policy.tool {
                Some(tool) if enabled => {
                    server.disabled_tools.remove(tool);
                }
                Some(tool) => {
                    server.disabled_tools.insert(tool.clone());
                }
                None => server.enabled = enabled,
            }
            Ok(())
        })?;
    crate::output::json(
        &mut std::io::stdout().lock(),
        &json!({"schema_version": 1, "actions": [{"server": policy.server, "tool": policy.tool, "enabled": enabled}], "shadowed_by_trusted_project": report.shadowed_by_trusted_project, "project_reapproval_required": report.project_reapproval_required}),
    )
}

fn target(policy: &Policy, paths: &Paths, effective: &Effective) -> Result<Server, Error> {
    let config: Configuration = if policy.scope.project {
        effective
            .project()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Configuration,
                    "project scope requires a discovered project",
                )
            })?
            .configuration()
            .clone()
    } else {
        crate::config::user_store(paths)?
            .read()?
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default()
    };
    config
        .servers
        .get(&policy.server)
        .cloned()
        .ok_or_else(unknown_server)
}

async fn validate_tool(
    policy: &Policy,
    target: &Server,
    paths: &Paths,
    cwd: &Path,
    effective: &Effective,
    operation: &Operation,
) -> Result<(), Error> {
    let tool: &str = policy.tool.as_deref().ok_or_else(unknown_tool)?;
    if target.disabled_tools.contains(tool) {
        return Ok(());
    }
    let server: crate::config::AuthorizedServer<'_> = effective.authorize(&policy.server)?;
    let project = matches!(
        server.identity().source,
        crate::config::Source::Project { .. }
    );
    if project != policy.scope.project || server.server() != target {
        return Err(Error::new(
            ErrorKind::Configuration,
            "tool target is shadowed; run outside the overriding project",
        ));
    }
    let value: Value = crate::supervisor::request(
        paths,
        cwd,
        &policy.server,
        &Action::Tools { all: true },
        operation,
    )
    .await?;
    if value["tools"]
        .as_array()
        .is_some_and(|tools| tools.iter().any(|entry| entry["name"] == tool))
    {
        return Ok(());
    }
    super::complete(&value)?;
    Err(unknown_tool())
}

fn unknown_server() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "server is not configured in target scope",
    )
}
fn unknown_tool() -> Error {
    Error::new(ErrorKind::Usage, "tool was not found in target server")
}
