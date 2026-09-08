pub mod call;
pub mod cli;
pub mod config;
pub mod deadline;
pub mod error;
pub mod json;
pub mod mcp;
pub mod output;
pub mod ownership;
pub mod storage;
pub mod trust;

use cli::{Cli, Command};
use error::{Error, ErrorKind};

pub fn dispatch(cli: Cli) -> Result<(), Error> {
    if cli.uninstall_everything && cli.command.is_some() {
        return Err(Error::new(
            ErrorKind::Usage,
            "self-uninstall cannot be combined with a command",
        ));
    }
    if cli.command.is_none() && !cli.uninstall_everything {
        return Err(Error::new(
            ErrorKind::Usage,
            "a command is required; use --help",
        ));
    }
    if !cfg!(target_os = "linux") {
        return Err(Error::new(ErrorKind::Unsupported, "v1 supports Linux only"));
    }
    match cli.command {
        Some(Command::Servers) => {
            let paths: storage::Paths = storage::Paths::from_environment();
            let cwd: std::path::PathBuf = std::env::current_dir().map_err(storage::io_error)?;
            let effective: config::Effective = config::Effective::load(&paths, &cwd)?;
            return output::json(
                &mut std::io::stdout().lock(),
                &ServersReport {
                    schema_version: 1,
                    servers: effective.reports(),
                },
            );
        }
        Some(Command::Trust) => {
            let cwd: std::path::PathBuf = std::env::current_dir().map_err(storage::io_error)?;
            return trust::run(&storage::Paths::from_environment(), &cwd, cli.timeout);
        }
        Some(Command::Call(call)) => {
            let _: call::Request = call.into_request()?;
        }
        _ => (),
    }
    Err(Error::new(
        ErrorKind::Unsupported,
        "operation is not implemented yet",
    ))
}

#[derive(serde::Serialize)]
struct ServersReport<'a> {
    schema_version: u32,
    servers: Vec<config::ServerReport<'a>>,
}
