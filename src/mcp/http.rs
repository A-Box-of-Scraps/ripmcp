use super::{
    Limits, Operation, PROTOCOL_VERSION, connection_error, limit_error, protocol, protocol_error,
    sse::Events,
};
use crate::error::{Error, ErrorKind};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

pub type AuthorizationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<HeaderValue>, Error>> + Send + 'a>>;

pub type ChallengeFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

pub trait AuthenticationProvider: Send + Sync {
    fn authorization<'a>(
        &'a self,
        resource: &'a Url,
        operation: &'a Operation,
    ) -> AuthorizationFuture<'a>;

    fn challenged<'a>(
        &'a self,
        _resource: &'a Url,
        _status: reqwest::StatusCode,
        _headers: &'a HeaderMap,
        _operation: &'a Operation,
    ) -> ChallengeFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

pub struct NoAuthentication;

impl AuthenticationProvider for NoAuthentication {
    fn authorization<'a>(&'a self, _: &'a Url, _: &'a Operation) -> AuthorizationFuture<'a> {
        Box::pin(async { Ok(None) })
    }
}

pub struct HttpOptions {
    pub endpoint: Url,
    pub headers: HeaderMap,
    pub authentication: Arc<dyn AuthenticationProvider>,
}

impl HttpOptions {
    pub fn new(endpoint: Url) -> Self {
        Self {
            endpoint,
            headers: HeaderMap::new(),
            authentication: Arc::new(NoAuthentication),
        }
    }
}

pub(super) struct Http {
    options: HttpOptions,
    client: reqwest::Client,
    permits: Semaphore,
    limits: Limits,
}

impl Http {
    pub(super) fn new(mut options: HttpOptions, limits: Limits) -> Result<Self, Error> {
        validate_endpoint(&options.endpoint)?;
        if !options.headers.is_empty() && options.endpoint.scheme() != "https" {
            return Err(Error::new(
                ErrorKind::Configuration,
                "configured credential headers require HTTPS",
            ));
        }
        for (name, value) in &mut options.headers {
            if reserved(name.as_str()) {
                return Err(Error::new(
                    ErrorKind::Configuration,
                    "HTTP header conflicts with MCP transport ownership",
                ));
            }
            value.set_sensitive(true);
        }
        let client: reqwest::Client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .referer(false)
            .http1_only()
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|_| connection_error())?;
        Ok(Self {
            options,
            client,
            permits: Semaphore::new(limits.in_flight),
            limits,
        })
    }

    pub(super) async fn request(
        &self,
        message: &Value,
        headers: HeaderMap,
        operation: &Operation,
    ) -> Result<Value, Error> {
        operation
            .run(async {
                let _permit: tokio::sync::SemaphorePermit<'_> = self
                    .permits
                    .acquire()
                    .await
                    .map_err(|_| connection_error())?;
                let authorization: Option<HeaderValue> = self
                    .options
                    .authentication
                    .authorization(&self.options.endpoint, operation)
                    .await?;
                if authorization.is_some() && self.options.endpoint.scheme() != "https" {
                    return Err(Error::new(
                        ErrorKind::Authentication,
                        "credentials require an HTTPS MCP endpoint",
                    ));
                }
                let body: Vec<u8> = protocol::encode(message, self.limits.frame_bytes)?;
                let mut request: reqwest::RequestBuilder = self
                    .client
                    .post(self.options.endpoint.clone())
                    .headers(self.options.headers.clone())
                    .headers(headers)
                    .header("accept", "application/json, text/event-stream")
                    .header("content-type", "application/json")
                    .header("mcp-protocol-version", PROTOCOL_VERSION)
                    .header(
                        "mcp-method",
                        message["method"].as_str().ok_or_else(protocol_error)?,
                    )
                    .body(body);
                if let Some(mut authorization) = authorization {
                    authorization.set_sensitive(true);
                    request = request.header("authorization", authorization);
                }
                let response: reqwest::Response =
                    request.send().await.map_err(|_| connection_error())?;
                self.response(
                    response,
                    message["id"].as_str().ok_or_else(protocol_error)?,
                    operation,
                )
                .await
            })
            .await
    }

    async fn response(
        &self,
        response: reqwest::Response,
        id: &str,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let status: reqwest::StatusCode = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            self.options
                .authentication
                .challenged(
                    &self.options.endpoint,
                    status,
                    response.headers(),
                    operation,
                )
                .await?;
            return Err(Error::new(
                ErrorKind::Authentication,
                "authentication required or denied; run ripmcp auth login <server> explicitly",
            ));
        }
        if status.is_redirection() {
            return Err(Error::new(
                ErrorKind::Connection,
                "MCP endpoint redirect refused; update and authorize the endpoint explicitly",
            ));
        }
        let media: String = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if media == "text/event-stream" && status.is_success() {
            return events(response, id, self.limits.frame_bytes).await;
        }
        if media != "application/json" {
            return Err(if status.is_success() {
                protocol_error()
            } else {
                connection_error()
            });
        }
        let bytes: Vec<u8> = bounded_body(response, self.limits.frame_bytes).await?;
        match protocol::decode(&bytes)? {
            protocol::Message::Response {
                id: response_id,
                result,
            } => {
                if response_id.as_deref() != Some(id)
                    && !(response_id.is_none() && !status.is_success() && result.is_err())
                {
                    return Err(protocol_error());
                }
                if !status.is_success() && result.is_ok() {
                    return Err(protocol_error());
                }
                result
            }
            protocol::Message::Notification => Err(protocol_error()),
        }
    }
}

async fn events(mut response: reqwest::Response, id: &str, limit: usize) -> Result<Value, Error> {
    let mut events: Events = Events::new(limit);
    while let Some(chunk) = response.chunk().await.map_err(|_| connection_error())? {
        if let Some(result) = events.feed(&chunk, id)? {
            return Ok(result);
        }
    }
    Err(connection_error())
}

async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, Error> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(limit_error());
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| connection_error())? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(limit_error());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_endpoint(endpoint: &Url) -> Result<(), Error> {
    let loopback = endpoint.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
        || !(endpoint.scheme() == "https" || endpoint.scheme() == "http" && loopback)
    {
        return Err(Error::new(
            ErrorKind::Configuration,
            "MCP requires HTTPS, or unauthenticated loopback HTTP, without URL credentials or fragments",
        ));
    }
    Ok(())
}

fn reserved(name: &str) -> bool {
    name.starts_with("mcp-")
        || matches!(
            name,
            "authorization"
                | "proxy-authorization"
                | "cookie"
                | "host"
                | "origin"
                | "referer"
                | "content-type"
                | "content-length"
                | "accept"
                | "connection"
                | "transfer-encoding"
                | "upgrade"
                | "te"
                | "trailer"
        )
}
