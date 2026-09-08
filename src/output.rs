use crate::error::{Error, ErrorKind};
use serde::Serialize;
use serde_json::Value;
use std::io::Write;

pub fn json(writer: &mut impl Write, value: &impl Serialize) -> Result<(), Error> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|_| Error::new(ErrorKind::Io, "cannot write JSON output"))?;
    writer
        .write_all(b"\n")
        .map_err(|_| Error::new(ErrorKind::Io, "cannot write output newline"))
}

pub fn tool_result(writer: &mut impl Write, envelope: &Value) -> Result<(), Error> {
    if !envelope.is_object()
        || envelope
            .get("isError")
            .is_some_and(|flag| !flag.is_boolean())
    {
        return Err(Error::new(
            ErrorKind::Protocol,
            "invalid tool-result envelope",
        ));
    }
    json(writer, envelope)?;
    if envelope.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(Error::new(
            ErrorKind::ToolResult,
            "tool reported failure; see JSON output",
        ));
    }
    Ok(())
}
