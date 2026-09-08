mod barrier;
mod client;
mod command;
mod endpoint;
mod guardian;
pub(crate) mod input;
pub(crate) mod launch;
mod manager;
mod service;
mod wire;

pub use barrier::Barrier;
pub use client::Connection;
pub use command::run as command;
pub use guardian::run as guard;
pub use manager::{Action, LocalRequest};

use crate::error::{Error, ErrorKind};

pub fn run() -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(service::serve())
}

fn invalid() -> Error {
    Error::new(ErrorKind::Configuration, "unsafe supervisor endpoint")
}

fn unavailable() -> Error {
    Error::new(ErrorKind::Connection, "supervisor connection closed")
}

fn incompatible() -> Error {
    Error::new(ErrorKind::Protocol, "incompatible supervisor IPC protocol")
}

pub(crate) use command::installation_environment;

pub(crate) use command::request;
