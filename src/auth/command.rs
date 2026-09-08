use super::{Provider, callback::SystemBrowser};
use crate::{
    cli::Auth,
    config::{AuthorizedServer, Definition, Effective, Identity, schema::Authentication},
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
    storage::Paths,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use url::Url;

#[derive(Serialize)]
struct Report<'a> {
    schema_version: u32,
    auth: AuthReport<'a>,
}

#[derive(Serialize)]
struct AuthReport<'a> {
    server: &'a str,
    state: &'static str,
    shared_by_endpoint: bool,
    remote_validity_checked: bool,
}

fn report<'a>(server: &'a str, state: &'static str) -> Report<'a> {
    Report {
        schema_version: 1,
        auth: AuthReport {
            server,
            state,
            shared_by_endpoint: true,
            remote_validity_checked: false,
        },
    }
}

pub fn run(command: Auth, timeout: Option<u64>) -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(execute(command, timeout))
}

async fn execute(command: Auth, timeout: Option<u64>) -> Result<(), Error> {
    let signal: SignalCancellation = SignalCancellation::install()?;
    let started: Instant = Instant::now();
    let paths: Paths = Paths::from_environment();
    let cwd: PathBuf = std::env::current_dir().map_err(crate::storage::io_error)?;
    let effective: Effective = Effective::load(&paths, &cwd)?;
    let name: &str = match &command {
        Auth::Login(server) | Auth::Status(server) | Auth::Logout(server) => &server.server,
    };
    let server: AuthorizedServer<'_> = effective.authorize(name)?;
    let endpoint: Url = endpoint(&server)?;
    let login = matches!(command, Auth::Login(_));
    if login {
        server.require_enabled(None)?;
    }
    let seconds = if login {
        effective.timeouts().login_seconds.get()
    } else {
        effective.timeouts().operation_seconds.get()
    };
    let operation: Operation = Operation::new(
        Deadline::new(
            Duration::from_secs(timeout.unwrap_or(seconds)).saturating_sub(started.elapsed()),
        ),
        signal.token(),
    );
    let provider: Provider = Provider::new(endpoint)?;
    let identity: Identity = server.identity().clone();
    let state: &str = match command {
        Auth::Login(_) => {
            eprintln!(
                "Preparing explicit OAuth login; secure storage and provider support are required. Press Ctrl-C to cancel."
            );
            provider
                .login(
                    &SystemBrowser,
                    || recheck(&paths, &cwd, &identity),
                    &operation,
                )
                .await?;
            "saved"
        }
        Auth::Status(_) => provider.status(&operation).await?,
        Auth::Logout(_) => {
            provider.logout(&operation).await?;
            "signed_out"
        }
    };
    let value: Report<'_> = report(&identity.name, state);
    crate::output::json(&mut std::io::stdout().lock(), &value)
}

fn endpoint(server: &AuthorizedServer<'_>) -> Result<Url, Error> {
    match &server.server().definition {
        Definition::Remote {
            url,
            authentication: Authentication::Oauth,
            headers,
            ..
        } => {
            if headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("authorization"))
            {
                return Err(Error::new(
                    ErrorKind::Configuration,
                    "OAuth cannot be combined with a configured Authorization header",
                ));
            }
            crate::config::schema::endpoint(url)
        }
        _ => Err(Error::new(
            ErrorKind::Unsupported,
            "auth commands require a remote server configured with OAuth; stdio credentials are separate",
        )),
    }
}

pub(super) fn recheck(
    paths: &Paths,
    cwd: &std::path::Path,
    identity: &Identity,
) -> Result<(), Error> {
    let effective: Effective = Effective::load(paths, cwd)?;
    let server: AuthorizedServer<'_> = effective.authorize(&identity.name)?;
    server.require_enabled(None)?;
    if server.identity() != identity {
        return Err(Error::new(
            ErrorKind::Configuration,
            "server configuration changed during operation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn auth_report_snapshots_have_only_public_status_fields() {
        for state in ["saved", "signed_out", "expired", "login_required"] {
            let mut bytes: Vec<u8> = Vec::new();
            crate::output::json(&mut bytes, &super::report("server", state)).unwrap();
            let expected: String = format!(
                "{{\"schema_version\":1,\"auth\":{{\"server\":\"server\",\"state\":\"{state}\",\"shared_by_endpoint\":true,\"remote_validity_checked\":false}}}}\n"
            );
            assert_eq!(String::from_utf8(bytes).unwrap(), expected);
        }
    }
}
