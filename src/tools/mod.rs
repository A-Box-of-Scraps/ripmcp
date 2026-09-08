pub(crate) mod policy;

use crate::{
    config::{AuthorizedServer, Effective},
    error::{Error, ErrorKind},
    mcp::{Discovery, Operation},
    storage::Paths,
    supervisor::Action,
};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn report(discovery: Discovery, server: &AuthorizedServer<'_>, all: bool) -> Value {
    let tools: Vec<Value> = discovery
        .tools
        .iter()
        .filter_map(|tool| {
            let enabled = !server.server().disabled_tools.contains(tool.name());
            if !all && !enabled {
                return None;
            }
            let mut value: Value =
                json!({"server": server.identity().name, "name": tool.name(), "enabled": enabled});
            for field in ["description", "title"] {
                if let Some(text) = tool.envelope().get(field) {
                    value[field] = text.clone();
                }
            }
            Some(value)
        })
        .collect();
    let errors: Vec<Value> = discovery.rejected_tools.iter().map(|name| json!({"server": server.identity().name, "tool": name, "code": 5, "message": "invalid tool definition"})).collect();
    json!({"schema_version": 1, "tools": tools, "errors": errors})
}

pub(crate) fn complete(value: &Value) -> Result<(), Error> {
    if value["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(Error::new(
            ErrorKind::PartialFailure,
            "incomplete tool discovery; uniqueness cannot be established",
        ));
    }
    Ok(())
}

async fn discover(
    paths: &Paths,
    cwd: &Path,
    effective: &Effective,
    operation: &Operation,
) -> Result<Value, Error> {
    let mut tools: Vec<Value> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();
    for server in effective
        .reports()
        .into_iter()
        .filter(|server| server.enabled)
    {
        match crate::supervisor::request(
            paths,
            cwd,
            server.name,
            &Action::Tools { all: false },
            operation,
        )
        .await
        {
            Ok(value) => {
                tools.extend(value["tools"].as_array().into_iter().flatten().cloned());
                errors.extend(value["errors"].as_array().into_iter().flatten().cloned());
            }
            Err(error) if error.kind == ErrorKind::Cancelled => return Err(error),
            Err(error) => errors.push(
                json!({"server": server.name, "code": error.kind as u8, "message": error.message}),
            ),
        }
    }
    let current: Effective = Effective::load(paths, cwd)?;
    let before: Vec<crate::config::Identity> = identities(effective)?;
    if before != identities(&current)? {
        errors.push(json!({"code": 3, "message": "configuration changed during discovery"}));
    }
    Ok(json!({"schema_version": 1, "tools": tools, "errors": errors}))
}

fn identities(effective: &Effective) -> Result<Vec<crate::config::Identity>, Error> {
    effective
        .reports()
        .into_iter()
        .map(|server| effective.identity(server.name).cloned())
        .collect()
}

pub(crate) async fn list(
    paths: &Paths,
    cwd: &Path,
    effective: &Effective,
    operation: &Operation,
) -> Result<(), Error> {
    let value: Value = discover(paths, cwd, effective, operation).await?;
    crate::output::json(&mut std::io::stdout().lock(), &value)?;
    complete(&value)
}

pub(crate) async fn resolve(
    paths: &Paths,
    cwd: &Path,
    effective: &Effective,
    tool: &str,
    operation: &Operation,
) -> Result<String, Error> {
    let value: Value = discover(paths, cwd, effective, operation).await?;
    if let Err(error) = complete(&value) {
        crate::output::json(&mut std::io::stdout().lock(), &value)?;
        return Err(error);
    }
    let candidates: Vec<String> = value["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["name"] == tool)
        .filter_map(|entry| entry["server"].as_str().map(str::to_owned))
        .collect();
    match candidates.as_slice() {
        [name] => Ok(name.clone()),
        [] => Err(Error::new(
            ErrorKind::Usage,
            "no enabled tool matches the requested name",
        )),
        _ => {
            let qualified: Vec<Value> = candidates
                .iter()
                .map(|server| json!({"server": server, "tool": tool}))
                .collect();
            crate::output::json(
                &mut std::io::stdout().lock(),
                &json!({"schema_version": 1, "candidates": qualified}),
            )?;
            Err(Error::new(
                ErrorKind::Usage,
                "ambiguous tool; retry with call <server> <tool>",
            ))
        }
    }
}
