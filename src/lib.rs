pub mod call;
pub mod cli;
pub mod error;
pub mod output;

use cli::{Cli, Command};
use error::{Error, ErrorKind};

pub fn dispatch(cli: Cli) -> Result<(), Error> {
    if cli.command.is_none() && !cli.uninstall_everything {
        return Err(Error::new(
            ErrorKind::Usage,
            "a command is required; use --help",
        ));
    }
    if let Some(Command::Call(call)) = cli.command {
        let _: call::Request = call.into_request()?;
    }
    if !cfg!(target_os = "linux") {
        return Err(Error::new(ErrorKind::Unsupported, "v1 supports Linux only"));
    }
    Err(Error::new(
        ErrorKind::Unsupported,
        "operation is not implemented yet",
    ))
}
