mod client;
mod endpoint;
mod service;
mod wire;

pub use client::Connection;
pub use service::Barrier;

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
