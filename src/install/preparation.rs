mod process;
mod resolve;

use crate::{
    config::schema::Runtime,
    error::{Error, ErrorKind},
    mcp::Operation,
    ownership::{Installation, Origin},
    storage::Paths,
};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) struct Context<'a> {
    pub paths: &'a Paths,
    pub target: &'a super::target::Target,
    pub environment: &'a BTreeMap<String, String>,
    pub installation: &'a Installation,
    pub operation: &'a Operation,
}

pub(crate) async fn prepare(context: &Context<'_>) -> Result<Origin, Error> {
    let Origin::Local {
        runtime,
        requested,
        executable: Some(executable),
        ..
    }: &Origin = &context.installation.origin
    else {
        return match &context.installation.origin {
            Origin::Remote { .. } => Ok(context.installation.origin.clone()),
            _ => Err(super::invalid()),
        };
    };
    let resolved: String = match runtime {
        Runtime::Npx => resolve::npx(context, executable, requested).await?,
        Runtime::Uvx => resolve::uvx(context, executable, requested).await?,
        Runtime::Docker => resolve::docker(context, executable, requested).await?,
    };
    if !crate::supervisor::launch::pinned(*runtime, &resolved) {
        return Err(super::invalid());
    }
    Ok(Origin::Local {
        runtime: *runtime,
        requested: requested.clone(),
        resolved: Some(resolved),
        executable: Some(executable.clone()),
    })
}

pub(crate) fn resolve_executable(
    installation: &mut Installation,
    environment: &BTreeMap<String, String>,
) -> Result<(), Error> {
    if let Origin::Local {
        runtime,
        executable: recorded,
        ..
    } = &mut installation.origin
    {
        *recorded = Some(executable(environment, name(*runtime))?);
    }
    Ok(())
}

pub(crate) fn name(runtime: Runtime) -> &'static str {
    match runtime {
        Runtime::Npx => "npx",
        Runtime::Uvx => "uvx",
        Runtime::Docker => "docker",
    }
}

fn executable(environment: &BTreeMap<String, String>, name: &str) -> Result<PathBuf, Error> {
    use std::os::unix::fs::PermissionsExt;
    let path: &String = environment.get("PATH").ok_or_else(missing)?;
    for directory in std::env::split_paths(path) {
        if !directory.is_absolute() {
            continue;
        }
        let candidate: PathBuf = directory.join(name);
        if std::fs::metadata(&candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        {
            // Keep the runtime's invocation name: uvx may be a symlink to uv.
            return Ok(candidate);
        }
    }
    Err(missing())
}
fn missing() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "required runtime executable is missing; install the runtime and retry",
    )
}
