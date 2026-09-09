use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Manage and invoke MCP servers")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[arg(long)]
    pub uninstall_everything: bool,
    #[arg(short = 'y', requires = "uninstall_everything")]
    pub yes: bool,
    #[arg(long, global = true, value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: Option<u64>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(name = "__supervisor", hide = true)]
    Supervisor,
    #[command(name = "__guard", hide = true)]
    Guard(Guard),
    Install(Install),
    Uninstall(Uninstall),
    Enable(Policy),
    Disable(Policy),
    Start(Server),
    Stop(Server),
    Servers,
    Trust,
    Tools(Tools),
    Tool(Tool),
    Call(Call),
    Shape(Shape),
    Auth {
        #[command(subcommand)]
        command: Auth,
    },
}

#[derive(Args)]
pub struct Guard {
    #[arg(long)]
    pub check_exit: bool,
    pub lease: String,
    pub parent: u32,
    #[arg(long)]
    pub runtime: PathBuf,
    #[arg(long)]
    pub state: PathBuf,
    #[arg(long)]
    pub installation: String,
    #[arg(long)]
    pub revision: String,
    #[arg(long)]
    pub container: Option<String>,
    #[arg(long = "container-env")]
    pub container_env: Vec<String>,
    #[arg(last = true, required = true)]
    pub command: Vec<std::ffi::OsString>,
}

#[derive(Args)]
pub struct Scope {
    #[arg(long, conflicts_with = "project")]
    pub user: bool,
    #[arg(long)]
    pub project: bool,
}

#[derive(Args)]
#[group(id = "source", required = true, multiple = false)]
pub struct InstallSource {
    #[arg(long)]
    pub npx: Option<String>,
    #[arg(long)]
    pub uvx: Option<String>,
    #[arg(long)]
    pub docker: Option<String>,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Args)]
pub struct Install {
    pub server: String,
    #[command(flatten)]
    pub source: InstallSource,
    #[command(flatten)]
    pub scope: Scope,
    #[arg(long)]
    pub skip_verify: bool,
    #[arg(last = true, conflicts_with = "config")]
    pub args: Vec<String>,
}

#[derive(Args)]
pub struct Uninstall {
    pub server: String,
    #[command(flatten)]
    pub scope: Scope,
    #[arg(long)]
    pub clean: bool,
    #[arg(short = 'y', requires = "clean")]
    pub yes: bool,
}

#[derive(Args)]
pub struct Policy {
    pub server: String,
    pub tool: Option<String>,
    #[command(flatten)]
    pub scope: Scope,
}

#[derive(Args)]
pub struct Server {
    pub server: String,
}

#[derive(Args)]
pub struct Tools {
    pub server: Option<String>,
    #[arg(long, requires = "server")]
    pub all: bool,
}

#[derive(Args)]
pub struct Tool {
    pub server: String,
    pub tool: String,
}

#[derive(Args)]
pub struct Call {
    #[arg(long, help = "Allow URL interaction and wait for the tool to finish")]
    pub interactive: bool,
    #[arg(required = true, num_args = 1..=3)]
    pub positionals: Vec<String>,
    #[arg(long)]
    pub input: Option<PathBuf>,
}

#[derive(Args)]
pub struct Shape {
    pub input: PathBuf,
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u32).range(1..=64))]
    pub depth: u32,
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub width: u32,
}

#[derive(Subcommand)]
pub enum Auth {
    Configure(Box<AuthConfigure>),
    Login(Server),
    Status(Server),
    Logout(Server),
}

#[derive(Args)]
pub struct AuthConfigure {
    pub server: String,
    #[command(flatten)]
    pub source: AuthSource,
    #[command(flatten)]
    pub scope: Scope,
    #[arg(long, requires = "header")]
    pub header_env: Option<String>,
    #[arg(long, requires = "oauth_client_id")]
    pub issuer: Option<String>,
    #[arg(
        long,
        requires = "oauth_client_id",
        conflicts_with = "client_secret_env"
    )]
    pub client_secret: bool,
    #[arg(long, requires = "oauth_client_id")]
    pub client_secret_env: Option<String>,
    #[arg(long, requires = "oauth_client_id")]
    pub token_endpoint_auth_method: Option<crate::config::authentication::ClientAuthMethod>,
    #[arg(long = "scope", requires = "oauth_client_id")]
    pub scopes: Vec<String>,
}

#[derive(Args)]
#[group(id = "auth_source", required = true, multiple = false)]
pub struct AuthSource {
    #[arg(long, help = "Prompt for a bearer token and save it in secure storage")]
    pub bearer: bool,
    #[arg(
        long,
        help = "Read a raw bearer token from this environment variable at invocation"
    )]
    pub bearer_env: Option<String>,
    #[arg(
        long,
        help = "Configure a custom credential header; prompt unless --header-env is given"
    )]
    pub header: Option<String>,
    #[arg(long, requires = "issuer")]
    pub oauth_client_id: Option<String>,
}

impl Cli {
    pub fn timeout_duration(&self) -> std::time::Duration {
        self.timeout_with(&crate::deadline::Timeouts::default())
    }

    pub fn timeout_with(&self, timeouts: &crate::deadline::Timeouts) -> std::time::Duration {
        let default_seconds = match &self.command {
            Some(Command::Auth {
                command: Auth::Login(_) | Auth::Configure(_),
            }) => timeouts.login_seconds.get(),
            _ => timeouts.operation_seconds.get(),
        };
        std::time::Duration::from_secs(self.timeout.unwrap_or(default_seconds))
    }
}
