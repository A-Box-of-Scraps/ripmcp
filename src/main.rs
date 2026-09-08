use clap::Parser;
use ripmcp::cli::Cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli: Cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if error.use_stderr() {
                eprintln!("ripmcp: invalid command arguments; use --help");
                return ExitCode::from(2);
            }
            return match error.print() {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(1),
            };
        }
    };
    match ripmcp::dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ripmcp: {error}");
            ExitCode::from(error.kind as u8)
        }
    }
}
