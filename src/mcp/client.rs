use super::{
    HttpOptions, Limits, Operation, PROTOCOL_VERSION, StderrLog, StdioOptions, connection_error,
    http::Http, protocol, protocol_error, stdio::Stdio,
};
use crate::error::{Error, ErrorKind};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

enum Transport {
    Stdio(Stdio),
    Http(Http),
}

pub struct Client {
    transport: Transport,
    next_id: AtomicU64,
    closed: AtomicBool,
    stop: tokio_util::sync::CancellationToken,
    pub(super) limits: Limits,
    server: Value,
}

impl Client {
    pub async fn stdio(
        options: StdioOptions,
        limits: Limits,
        operation: &Operation,
    ) -> Result<Self, Error> {
        operation.remaining()?;
        let limits: Limits = limits.validate()?;
        Self::connect(
            Transport::Stdio(Stdio::spawn(options, limits)?),
            limits,
            operation,
        )
        .await
    }

    pub async fn http(
        options: HttpOptions,
        limits: Limits,
        operation: &Operation,
    ) -> Result<Self, Error> {
        operation.remaining()?;
        let limits: Limits = limits.validate()?;
        Self::connect(
            Transport::Http(Http::new(options, limits)?),
            limits,
            operation,
        )
        .await
    }

    async fn connect(
        transport: Transport,
        limits: Limits,
        operation: &Operation,
    ) -> Result<Self, Error> {
        let mut client: Self = Self {
            transport,
            next_id: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            stop: tokio_util::sync::CancellationToken::new(),
            limits,
            server: Value::Null,
        };
        let result: Result<Value, Error> = client
            .request("server/discover", json!({}), HeaderMap::new(), operation)
            .await;
        match result.and_then(validate_server).and_then(|server| {
            operation.remaining()?;
            Ok(server)
        }) {
            Ok(server) => {
                client.server = server;
                Ok(client)
            }
            Err(error) => {
                client.shutdown().await;
                Err(error)
            }
        }
    }

    pub fn server_info(&self) -> &Value {
        &self.server
    }

    pub fn stderr_log(&self) -> Option<StderrLog> {
        match &self.transport {
            Transport::Stdio(transport) => Some(transport.stderr_log()),
            Transport::Http(_) => None,
        }
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.stop.cancel();
        if let Transport::Stdio(transport) = &self.transport {
            transport.shutdown().await;
        }
    }

    pub(super) fn is_http(&self) -> bool {
        matches!(self.transport, Transport::Http(_))
    }

    pub(super) async fn request(
        &self,
        method: &str,
        params: Value,
        headers: HeaderMap,
        operation: &Operation,
    ) -> Result<Value, Error> {
        operation.remaining()?;
        if self.closed.load(Ordering::Acquire) {
            return Err(connection_error());
        }
        let number: u64 = self
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| super::limit_error())?;
        let message: Value = protocol::request(&format!("ripmcp-{number}"), method, params);
        tokio::select! {
            biased;
            () = self.stop.cancelled() => Err(connection_error()),
            result = async {
                match &self.transport {
                    Transport::Stdio(transport) => transport.request(&message, operation).await,
                    Transport::Http(transport) => transport.request(&message, headers, operation).await,
                }
            } => result,
        }
    }
}

fn validate_server(server: Value) -> Result<Value, Error> {
    protocol::complete(&server)?;
    let versions: &Vec<Value> = server["supportedVersions"]
        .as_array()
        .ok_or_else(protocol_error)?;
    if versions.iter().any(|version| !version.is_string()) || !server["capabilities"].is_object() {
        return Err(protocol_error());
    }
    if !versions.iter().any(|version| version == PROTOCOL_VERSION) {
        return Err(Error::new(
            ErrorKind::Protocol,
            "server does not support MCP 2026-07-28; legacy fallback is disabled",
        ));
    }
    match server["capabilities"].get("tools") {
        Some(Value::Object(_)) => Ok(server),
        None => Err(Error::new(
            ErrorKind::Unsupported,
            "MCP server does not advertise tools support",
        )),
        _ => Err(protocol_error()),
    }
}
