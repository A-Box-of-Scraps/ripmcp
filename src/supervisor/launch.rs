use crate::config::{AuthorizedServer, Definition, schema::Runtime};
use crate::error::{Error, ErrorKind};
use crate::mcp::StdioOptions;
use crate::ownership::{
    Installation, InstallationId, Journal, Origin, OwnershipStore, Registration,
};
use crate::storage::{Directory, Paths};
use crate::trust::secrets::{EnvironmentOnly, Secret, SecretBackend};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct Prepared {
    pub options: StdioOptions,
    pub revision: String,
    pub configuration_digest: String,
    pub installation: InstallationId,
}

impl Prepared {
    pub async fn load(
        server: &AuthorizedServer<'_>,
        paths: &Paths,
        environment: &BTreeMap<String, String>,
        key: &str,
        operation: &crate::mcp::Operation,
    ) -> Result<Self, Error> {
        let journal: Journal = OwnershipStore::new(paths)?.read()?;
        let installation: &Installation = installation(&journal, server)?;
        Self::for_installation(server, paths, environment, key, operation, installation).await
    }

    pub(crate) async fn for_installation(
        server: &AuthorizedServer<'_>,
        paths: &Paths,
        environment: &BTreeMap<String, String>,
        key: &str,
        operation: &crate::mcp::Operation,
        installation: &Installation,
    ) -> Result<Self, Error> {
        server.require_enabled(None)?;
        let Definition::Local {
            runtime,
            package,
            args,
            env: references,
            cwd,
            ..
        }: &Definition = &server.server().definition
        else {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "remote servers have no local lifecycle",
            ));
        };
        let (resolved, executable): (&str, &PathBuf) = resolution(installation, *runtime, package)?;
        let mut env: BTreeMap<OsString, OsString> =
            resolved_environment(server, environment, operation).await?;
        if *runtime == Runtime::Uvx {
            env.insert("UV_PYTHON_DOWNLOADS".into(), "never".into());
        }
        let command: Vec<OsString> = runtime_command(*runtime, executable, resolved, args)?;
        let revision: String = revision(server, installation, executable, &env)?;
        let directory: Directory = paths.runtime(true)?.ok_or_else(super::invalid)?;
        let root: PathBuf =
            std::fs::read_link(directory.descriptor_path()).map_err(crate::storage::io_error)?;
        let mut options: StdioOptions =
            StdioOptions::new(std::env::current_exe().map_err(crate::storage::io_error)?);
        options.args = vec![
            "__guard".into(),
            key.into(),
            std::process::id().to_string().into(),
            "--runtime".into(),
            root.into_os_string(),
            "--state".into(),
            paths
                .directory(crate::storage::Location::State)?
                .into_os_string(),
            "--installation".into(),
            installation.id().as_str().into(),
            "--revision".into(),
            revision.clone().into(),
        ];
        if *runtime == Runtime::Docker {
            options.args.extend([
                OsString::from("--container"),
                InstallationId::new()?.as_str().into(),
            ]);
            for name in references.keys() {
                options
                    .args
                    .extend([OsString::from("--container-env"), name.into()]);
            }
        }
        options.args.push("--".into());
        options.args.extend(command);
        options.env = env;
        options.cwd = cwd.clone().or_else(|| match &server.identity().source {
            crate::config::Source::Project { root } => Some(root.clone()),
            crate::config::Source::User { .. } => Some(PathBuf::from("/")),
        });
        options.termination_grace = Duration::from_secs(8);
        Ok(Self {
            options,
            revision,
            configuration_digest: server.identity().configuration_digest.clone(),
            installation: installation.id().clone(),
        })
    }
}

fn runtime_command(
    runtime: Runtime,
    executable: &std::path::Path,
    resolved: &str,
    args: &[String],
) -> Result<Vec<OsString>, Error> {
    let mut command: Vec<OsString> = vec![executable.as_os_str().to_owned()];
    match runtime {
        Runtime::Npx => command.extend([OsString::from("--yes"), resolved.into()]),
        Runtime::Uvx => {
            let (package, _): (&str, &str) = resolved.split_once("==").ok_or_else(unprepared)?;
            command.extend([OsString::from("--from"), resolved.into(), package.into()]);
        }
        Runtime::Docker => command.push(resolved.into()),
    }
    command.extend(args.iter().map(OsString::from));
    Ok(command)
}

async fn resolved_environment(
    server: &AuthorizedServer<'_>,
    environment: &BTreeMap<String, String>,
    operation: &crate::mcp::Operation,
) -> Result<BTreeMap<OsString, OsString>, Error> {
    let mut env: BTreeMap<OsString, OsString> = environment
        .iter()
        .filter(|(name, _)| BASE_ENV.contains(&name.as_str()))
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    let secrets: BTreeMap<String, Secret> = server
        .resolve_secrets_async(&References(environment), operation)
        .await?;
    for (key, secret) in secrets {
        env.insert(key.into(), secret.expose().into());
    }
    Ok(env)
}

pub(crate) const BASE_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
];

fn installation<'a>(
    journal: &'a Journal,
    server: &AuthorizedServer<'_>,
) -> Result<&'a Installation, Error> {
    let mut matches: std::vec::IntoIter<&Installation> = journal
        .installations
        .values()
        .filter(|entry| {
            entry.registration == Registration::Registered
                && entry.scope == server.identity().source
                && entry.server == server.identity().name
        })
        .collect::<Vec<_>>()
        .into_iter();
    let entry: &Installation = matches.next().ok_or_else(unprepared)?;
    if matches.next().is_some() {
        return Err(unprepared());
    }
    if journal.operations.values().any(|operation| {
        operation.installation_id == *entry.id()
            && matches!(operation.kind, crate::ownership::OperationKind::Install)
            && !matches!(operation.state, crate::ownership::OperationState::Committed)
    }) && entry.configuration_digest.as_ref() != Some(&server.identity().configuration_digest)
    {
        return Err(unprepared());
    }
    Ok(entry)
}

fn resolution<'a>(
    entry: &'a Installation,
    runtime: Runtime,
    package: &str,
) -> Result<(&'a str, &'a PathBuf), Error> {
    let Origin::Local {
        runtime: recorded,
        requested,
        resolved: Some(resolved),
        executable: Some(executable),
    }: &Origin = &entry.origin
    else {
        return Err(unprepared());
    };
    if *recorded != runtime
        || requested != package
        || !executable.is_absolute()
        || !pinned(runtime, resolved)
    {
        return Err(unprepared());
    }
    Ok((resolved, executable))
}

pub(crate) fn pinned(runtime: Runtime, resolved: &str) -> bool {
    if resolved.starts_with('-') || resolved.chars().any(char::is_whitespace) {
        return false;
    }
    match runtime {
        Runtime::Docker => resolved
            .rsplit_once("@sha256:")
            .is_some_and(|(name, hash)| {
                !name.is_empty()
                    && hash.len() == 64
                    && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            }),
        Runtime::Npx | Runtime::Uvx => {
            let version: Option<(&str, &str)> = if runtime == Runtime::Npx {
                resolved.rsplit_once('@')
            } else {
                resolved.split_once("==")
            };
            version.is_some_and(|(name, version)| {
                !name.is_empty()
                    && (runtime != Runtime::Npx || exact_npm_version(version))
                    && version.as_bytes().first().is_some_and(u8::is_ascii_digit)
                    && version
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-+_!".contains(&byte))
            })
        }
    }
}

fn revision(
    server: &AuthorizedServer<'_>,
    installation: &Installation,
    executable: &PathBuf,
    env: &BTreeMap<OsString, OsString>,
) -> Result<String, Error> {
    let metadata: std::fs::Metadata = std::fs::metadata(executable).map_err(|_| unprepared())?;
    if !metadata.is_file() || metadata.mode() & 0o111 == 0 {
        return Err(unprepared());
    }
    let environment: Vec<(&OsString, &OsString)> = env.iter().collect();
    let bytes: Vec<u8> = serde_json::to_vec(&(
        server.identity(),
        installation.id(),
        &installation.origin,
        environment,
        (
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        ),
    ))
    .map_err(crate::storage::io_error)?;
    Ok(crate::config::digest(&bytes))
}

fn exact_npm_version(version: &str) -> bool {
    let (release, build): (&str, Option<&str>) = version
        .split_once('+')
        .map_or((version, None), |(release, build)| (release, Some(build)));
    let (core, pre): (&str, Option<&str>) = release
        .split_once('-')
        .map_or((release, None), |(core, pre)| (core, Some(pre)));
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|number| {
            !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
                && (number.len() == 1 || !number.starts_with('0'))
        })
        && [pre, build].into_iter().flatten().all(|suffix| {
            suffix.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        })
}

pub(crate) struct References<'a>(pub &'a BTreeMap<String, String>);
impl SecretBackend for References<'_> {
    fn environment(&self, name: &str) -> Result<Secret, Error> {
        self.0.get(name).cloned().map(Secret::new).ok_or_else(|| {
            Error::new(
                ErrorKind::Authentication,
                "required environment secret is unavailable",
            )
        })
    }
    fn keyring(&self, id: &str) -> Result<Secret, Error> {
        EnvironmentOnly.keyring(id)
    }
    fn keyring_async<'a>(&'a self, id: &'a str) -> crate::auth::StoreFuture<'a, Secret> {
        EnvironmentOnly.keyring_async(id)
    }
}

fn unprepared() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "local server requires a matching immutable prepared installation and runtime executable",
    )
}

#[cfg(test)]
mod tests {
    use super::pinned;
    use crate::config::schema::Runtime;

    #[test]
    fn runtime_resolution_rejects_floating_tags_and_ranges() {
        for version in [
            "fixture@latest",
            "fixture@1moving",
            "fixture@^1.2.3",
            "fixture@1.2.3-",
            "fixture@1.2",
            "fixture@01.2.3",
        ] {
            assert!(!pinned(Runtime::Npx, version));
        }
        assert!(pinned(Runtime::Npx, "@scope/fixture@1.2.3-beta.1+build"));
        assert!(pinned(Runtime::Uvx, "fixture==1.2.3"));
        assert!(!pinned(Runtime::Uvx, "fixture==1.*"));
        assert!(!pinned(Runtime::Docker, "fixture:latest"));
    }
}
