pub mod auth;
pub mod call;
pub mod cleanup;
pub mod cli;
pub mod config;
pub mod deadline;
pub mod error;
pub mod install;
pub mod json;
pub mod mcp;
pub mod output;
pub mod ownership;
pub mod shape;
pub mod storage;
pub mod supervisor;
pub mod tools;
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
    if cli.uninstall_everything {
        return cleanup::uninstall_everything(cli.yes, cli.timeout);
    }
    match cli.command {
        Some(Command::Shape(shape)) => return shape::run(shape, cli.timeout),
        Some(Command::Install(install)) => return install::run(install, cli.timeout),
        Some(Command::Uninstall(uninstall)) => return cleanup::run(uninstall, cli.timeout),
        Some(Command::Auth { command }) => return auth::run(command, cli.timeout),
        Some(Command::Supervisor) => return supervisor::run(),
        Some(Command::Guard(guard)) => return supervisor::guard(guard),
        Some(
            command @ (Command::Servers
            | Command::Start(_)
            | Command::Stop(_)
            | Command::Enable(_)
            | Command::Disable(_)
            | Command::Tools(_)
            | Command::Tool(_)
            | Command::Call(_)),
        ) => return supervisor::command(command, cli.timeout),
        Some(Command::Trust) => {
            let cwd: std::path::PathBuf = std::env::current_dir().map_err(storage::io_error)?;
            return trust::run(&storage::Paths::from_environment(), &cwd, cli.timeout);
        }
        _ => (),
    }
    Err(Error::new(
        ErrorKind::Unsupported,
        "operation is not implemented yet",
    ))
}
