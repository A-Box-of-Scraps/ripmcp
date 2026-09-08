use crate::{
    cli::Shape,
    deadline::Deadline,
    error::{Error, ErrorKind},
    mcp::{Operation, SignalCancellation},
};
use serde_json::{Map, Value, json};
use std::time::Duration;

pub fn run(shape: Shape, timeout: Option<u64>) -> Result<(), Error> {
    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::storage::io_error)?;
    runtime.block_on(async {
        let signal: SignalCancellation = SignalCancellation::install()?;
        let operation: Operation = Operation::new(Deadline::new(Duration::from_secs(timeout.unwrap_or(60))), signal.token());
        let bytes: Vec<u8> = crate::supervisor::input::read(shape.input, 64 * 1024 * 1024, &operation).await?;
        let value: Value = crate::json::parse(&bytes).map_err(|_| Error::new(ErrorKind::Usage, "expected valid JSON within the shape size and nesting limits"))?;
        let mut remaining: usize = 100_000;
        let report: Value = inspect(&value, shape.depth, shape.width as usize, &mut remaining);
        operation.remaining()?;
        crate::output::json(&mut std::io::stdout().lock(), &json!({"schema_version": 1, "shape": report, "limits": {"bytes": 67108864, "nesting": 128, "nodes": 100000, "depth": shape.depth, "width": shape.width}}))
    })
}

fn inspect(value: &Value, depth: u32, width: usize, remaining: &mut usize) -> Value {
    *remaining -= 1;
    let kind: &str = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let mut report: Value = json!({"type": kind});
    match value {
        Value::Object(fields) => {
            let mut entries: Map<String, Value> = Map::new();
            for (name, value) in fields.iter().take(width) {
                if depth == 0 || *remaining == 0 {
                    break;
                }
                entries.insert(name.clone(), inspect(value, depth - 1, width, remaining));
            }
            omitted(&mut report, fields.len() - entries.len());
            report["fields"] = Value::Object(entries);
        }
        Value::Array(items) => {
            let mut entries: Vec<Value> = Vec::new();
            for value in items.iter().take(width) {
                if depth == 0 || *remaining == 0 {
                    break;
                }
                entries.push(inspect(value, depth - 1, width, remaining));
            }
            omitted(&mut report, items.len() - entries.len());
            report["length"] = json!(items.len());
            report["items"] = Value::Array(entries);
        }
        _ => (),
    }
    report
}

fn omitted(report: &mut Value, count: usize) {
    report["truncated"] = json!(count != 0);
    if count != 0 {
        report["omitted"] = json!(count);
    }
}
