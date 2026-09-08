use super::{Context, executable, process::run};
use crate::{config::schema::Runtime, error::Error};
use std::path::{Path, PathBuf};

pub(super) async fn npx(
    context: &Context<'_>,
    runtime: &Path,
    requested: &str,
) -> Result<String, Error> {
    let package: &str = npm_name(requested)?;
    let npm: PathBuf = executable(context.environment, "npm")?;
    let node: PathBuf = executable(context.environment, "node")?;
    let output: String = run(context, &npm, &["view", requested, "version", "--json"]).await?;
    let value: serde_json::Value =
        crate::json::parse(output.as_bytes()).map_err(|_| crate::install::invalid())?;
    let version: &str = match &value {
        serde_json::Value::String(version) => version,
        serde_json::Value::Array(versions) => versions
            .last()
            .and_then(serde_json::Value::as_str)
            .ok_or_else(crate::install::invalid)?,
        _ => return Err(crate::install::invalid()),
    };
    let resolved: String = format!("{package}@{version}");
    if !crate::supervisor::launch::pinned(Runtime::Npx, &resolved) {
        return Err(crate::install::invalid());
    }
    if crate::supervisor::launch::pinned(Runtime::Npx, requested) && requested != resolved {
        return Err(crate::install::invalid());
    }
    let _: String = run(
        context,
        runtime,
        &[
            "--yes",
            "--package",
            &resolved,
            "--",
            node.to_str().ok_or_else(crate::install::invalid)?,
            "-e",
            "",
        ],
    )
    .await?;
    Ok(resolved)
}

fn npm_name(requested: &str) -> Result<&str, Error> {
    let package: &str = match requested.rsplit_once('@') {
        Some((package, version)) if !package.is_empty() && !version.is_empty() => package,
        _ => requested,
    };
    let unscoped: &str = package.strip_prefix('@').unwrap_or(package);
    if unscoped.is_empty()
        || !unscoped
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._/".contains(&byte))
        || unscoped.contains('/') != package.starts_with('@')
        || unscoped.matches('/').count() > 1
        || requested.chars().any(char::is_whitespace)
        || requested.contains(':')
    {
        return Err(crate::install::invalid());
    }
    Ok(package)
}

pub(super) async fn uvx(
    context: &Context<'_>,
    runtime: &Path,
    requested: &str,
) -> Result<String, Error> {
    let (package, version): (&str, Option<&str>) = requested
        .split_once("==")
        .or_else(|| requested.split_once('@'))
        .map_or((requested, None), |(name, version)| (name, Some(version)));
    let requirement: String = match version {
        None | Some("latest") => package.to_owned(),
        Some(version) => format!("{package}=={version}"),
    };
    if package.is_empty()
        || !package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        || (requirement != package
            && !crate::supervisor::launch::pinned(Runtime::Uvx, &requirement))
    {
        return Err(crate::install::invalid());
    }
    let script = "import importlib.metadata,sys; print(importlib.metadata.version(sys.argv[1]))";
    let output: String = run(
        context,
        runtime,
        &[
            "--no-python-downloads",
            "--refresh",
            "--from",
            &requirement,
            "python",
            "-I",
            "-c",
            script,
            package,
        ],
    )
    .await?;
    let resolved: String = format!("{package}=={}", output.trim());
    if requirement != package && resolved != requirement {
        return Err(crate::install::invalid());
    }
    Ok(resolved)
}

pub(super) async fn docker(
    context: &Context<'_>,
    runtime: &Path,
    requested: &str,
) -> Result<String, Error> {
    if requested.starts_with('-') || requested.chars().any(char::is_whitespace) {
        return Err(crate::install::invalid());
    }
    let _: String = run(context, runtime, &["pull", requested]).await?;
    let output: String = run(
        context,
        runtime,
        &[
            "image",
            "inspect",
            "--format",
            "{{json .RepoDigests}}",
            requested,
        ],
    )
    .await?;
    let value: serde_json::Value =
        crate::json::parse(output.as_bytes()).map_err(|_| crate::install::invalid())?;
    let digests: &Vec<serde_json::Value> = value.as_array().ok_or_else(crate::install::invalid)?;
    if requested.contains('@') {
        if !digests
            .iter()
            .any(|digest| digest.as_str() == Some(requested))
        {
            return Err(crate::install::invalid());
        }
        return Ok(requested.to_owned());
    }
    // Inspect the just-pulled image, then launch only its recorded repository digest.
    let resolved: &str = digests
        .first()
        .and_then(serde_json::Value::as_str)
        .ok_or_else(crate::install::invalid)?;
    Ok(resolved.to_owned())
}
