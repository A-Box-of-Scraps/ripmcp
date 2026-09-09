use super::{PROTOCOL_VERSION, limit_error, protocol_error};
use crate::error::{Error, ErrorKind};
use serde_json::{Value, json};

pub(super) enum Message {
    Response {
        id: Option<String>,
        result: Result<Value, Error>,
    },
    Notification,
}

pub(super) fn request(id: &str, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "ripmcp", "version": env!("CARGO_PKG_VERSION")}
    });
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

pub(super) fn encode(value: &Value, limit: usize) -> Result<Vec<u8>, Error> {
    let mut writer: Bounded = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| limit_error())?;
    Ok(writer.bytes)
}

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("MCP frame limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Message, Error> {
    let value: Value = crate::json::parse(bytes).map_err(|_| protocol_error())?;
    if value["jsonrpc"] != "2.0" || !value.is_object() {
        return Err(protocol_error());
    }
    if value.get("method").is_some() {
        return notification(&value);
    }
    let id: Option<String> = match value.get("id") {
        Some(Value::String(id)) => Some(id.clone()),
        None if value.get("error").is_some() => None,
        _ => return Err(protocol_error()),
    };
    match (value.get("result"), value.get("error")) {
        (Some(result), None) if id.is_some() && result.is_object() => Ok(Message::Response {
            id,
            result: Ok(result.clone()),
        }),
        (None, Some(error)) => Ok(Message::Response {
            id,
            result: Err(rpc_error(error)?),
        }),
        _ => Err(protocol_error()),
    }
}

fn notification(value: &Value) -> Result<Message, Error> {
    if value.get("id").is_some() {
        return Err(Error::new(
            ErrorKind::Protocol,
            "server-initiated requests are forbidden by MCP 2026-07-28",
        ));
    }
    if !value["method"].is_string()
        || value.get("result").is_some()
        || value.get("error").is_some()
        || value
            .get("params")
            .is_some_and(|params| !params.is_object())
    {
        return Err(protocol_error());
    }
    Ok(Message::Notification)
}

fn rpc_error(value: &Value) -> Result<Error, Error> {
    let code: i64 = value["code"].as_i64().ok_or_else(protocol_error)?;
    if !value["message"].is_string() {
        return Err(protocol_error());
    }
    Ok(match code {
        -32022 => Error::new(
            ErrorKind::Protocol,
            "server does not support MCP 2026-07-28; legacy fallback is disabled",
        ),
        -32021 => Error::new(
            ErrorKind::Unsupported,
            "server requires an unsupported client capability",
        ),
        _ => Error::new(ErrorKind::Protocol, "MCP server returned a protocol error"),
    })
}

pub(super) fn complete(value: &Value) -> Result<(), Error> {
    if value.get("_meta").is_some_and(|meta| !meta.is_object()) {
        return Err(protocol_error());
    }
    match value["resultType"].as_str() {
        Some("complete") => Ok(()),
        Some("input_required") => Err(Error::new(
            ErrorKind::Unsupported,
            "tool requires client input; use --interactive for URL interaction; invocation was not replayed",
        )),
        Some(_) => Err(Error::new(
            ErrorKind::Unsupported,
            "MCP result type is not supported",
        )),
        _ => Err(protocol_error()),
    }
}
