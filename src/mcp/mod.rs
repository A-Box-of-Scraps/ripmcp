mod client;
mod headers;
mod http;
mod operation;
mod protocol;
mod sse;
mod stdio;
mod tools;

pub use client::Client;
pub use http::{
    AuthenticationProvider, AuthorizationFuture, ChallengeFuture, HttpOptions, NoAuthentication,
};
pub use operation::{Operation, SignalCancellation};
pub use stdio::{StderrLog, StdioOptions};
pub use tokio_util::sync::CancellationToken;
pub use tools::{Discovery, Tool, ToolResult};

use crate::error::{Error, ErrorKind};

pub const PROTOCOL_VERSION: &str = "2026-07-28";

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub frame_bytes: usize,
    pub discovery_bytes: usize,
    pub tools: usize,
    pub pages: usize,
    pub in_flight: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: 16 * 1024 * 1024,
            discovery_bytes: 32 * 1024 * 1024,
            tools: 10_000,
            pages: 1_000,
            in_flight: 128,
        }
    }
}

impl Limits {
    fn validate(self) -> Result<Self, Error> {
        if self.frame_bytes == 0
            || self.discovery_bytes == 0
            || self.tools == 0
            || self.pages == 0
            || self.in_flight == 0
            || self.in_flight > 65_536
        {
            return Err(Error::new(ErrorKind::Configuration, "invalid MCP limits"));
        }
        Ok(self)
    }
}

fn protocol_error() -> Error {
    Error::new(ErrorKind::Protocol, "invalid MCP message")
}

fn connection_error() -> Error {
    Error::new(
        ErrorKind::Connection,
        "MCP transport closed; completion may be unknown",
    )
}

fn limit_error() -> Error {
    Error::new(
        ErrorKind::Protocol,
        "MCP resource limit exceeded; no truncated result was returned",
    )
}
