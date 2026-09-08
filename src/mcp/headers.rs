use super::{protocol_error, tools::Tool};
use crate::error::{Error, ErrorKind};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct Parameter {
    path: Vec<String>,
    name: HeaderName,
    kind: String,
}

pub(super) fn annotations(schema: &Value) -> Result<Vec<Parameter>, Error> {
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut names: HashSet<HeaderName> = HashSet::new();
    walk(schema, &[], true, &mut parameters, &mut names)?;
    Ok(parameters)
}

fn walk(
    schema: &Value,
    path: &[String],
    reachable: bool,
    parameters: &mut Vec<Parameter>,
    names: &mut HashSet<HeaderName>,
) -> Result<(), Error> {
    match schema {
        Value::Object(object) => {
            if let Some(annotation) = object.get("x-mcp-header") {
                let parameter: Parameter =
                    annotation_parameter(schema, annotation, path, reachable)?;
                if !names.insert(parameter.name.clone()) {
                    return Err(protocol_error());
                }
                parameters.push(parameter);
            }
            for (key, value) in object {
                walk_child(key, value, path, reachable, parameters, names)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                walk(value, path, false, parameters, names)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn walk_child(
    key: &str,
    value: &Value,
    path: &[String],
    reachable: bool,
    parameters: &mut Vec<Parameter>,
    names: &mut HashSet<HeaderName>,
) -> Result<(), Error> {
    if key == "properties" && reachable {
        let properties: &serde_json::Map<String, Value> =
            value.as_object().ok_or_else(protocol_error)?;
        for (name, property) in properties {
            let mut child: Vec<String> = path.to_vec();
            child.push(name.clone());
            walk(property, &child, true, parameters, names)?;
        }
    } else {
        walk(value, path, false, parameters, names)?;
    }
    Ok(())
}

fn annotation_parameter(
    schema: &Value,
    annotation: &Value,
    path: &[String],
    reachable: bool,
) -> Result<Parameter, Error> {
    let suffix: &str = annotation
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(protocol_error)?;
    let name: HeaderName = HeaderName::from_bytes(format!("Mcp-Param-{suffix}").as_bytes())
        .map_err(|_| protocol_error())?;
    let kind: &str = schema["type"].as_str().ok_or_else(protocol_error)?;
    if !reachable || path.is_empty() || !matches!(kind, "string" | "integer" | "boolean") {
        return Err(protocol_error());
    }
    Ok(Parameter {
        path: path.to_vec(),
        name,
        kind: kind.to_owned(),
    })
}

pub(super) fn tool_headers(tool: &Tool, arguments: &Value) -> Result<HeaderMap, Error> {
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert("mcp-name", encoded(tool.name())?);
    for parameter in annotations(tool.input_schema())? {
        let mut value: Option<&Value> = Some(arguments);
        for key in parameter.path {
            value = value.and_then(|parent| parent.get(&key));
        }
        if let Some(value) = value.filter(|value| !value.is_null()) {
            headers.insert(
                parameter.name,
                encoded(&primitive(value, &parameter.kind)?)?,
            );
        }
    }
    Ok(headers)
}

fn primitive(value: &Value, kind: &str) -> Result<String, Error> {
    match (kind, value) {
        ("string", Value::String(value)) => Ok(value.clone()),
        ("boolean", Value::Bool(value)) => Ok(value.to_string()),
        ("integer", Value::Number(value)) => integer(value),
        _ => Err(argument_error()),
    }
}

fn integer(value: &serde_json::Number) -> Result<String, Error> {
    // Parse decimal precision exactly; f64 can round a fractional argument into an integer.
    let raw: String = value.to_string();
    let unsigned: &str = raw.strip_prefix('-').unwrap_or(&raw);
    let (mantissa, exponent): (&str, i64) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse().map_err(|_| argument_error())?),
        None => (unsigned, 0),
    };
    let (whole, fraction): (&str, &str) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits: String = format!("{whole}{fraction}");
    let significant: &str = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok("0".to_owned());
    }
    let trimmed: &str = significant.trim_end_matches('0');
    let trailing =
        i64::try_from(significant.len() - trimmed.len()).map_err(|_| argument_error())?;
    let decimals = i64::try_from(fraction.len()).map_err(|_| argument_error())?;
    let shift = exponent
        .checked_sub(decimals)
        .and_then(|shift| shift.checked_add(trailing))
        .ok_or_else(argument_error)?;
    if !(0..=16).contains(&shift) || trimmed.len() + shift as usize > 16 {
        return Err(argument_error());
    }
    let canonical: String = format!(
        "{}{}{}",
        if raw.starts_with('-') { "-" } else { "" },
        trimmed,
        "0".repeat(shift as usize)
    );
    let number: i64 = canonical.parse().map_err(|_| argument_error())?;
    if number.abs() > 9_007_199_254_740_991 {
        return Err(argument_error());
    }
    Ok(canonical)
}

fn argument_error() -> Error {
    Error::new(
        ErrorKind::Usage,
        "tool argument cannot be mirrored into its MCP header",
    )
}

pub(super) fn encoded(value: &str) -> Result<HeaderValue, Error> {
    let safe = value.trim() == value
        && value
            .bytes()
            .all(|byte| (0x20..=0x7e).contains(&byte) || byte == b'\t')
        && !(value.starts_with("=?base64?") && value.ends_with("?="));
    let mut header: HeaderValue = if safe {
        HeaderValue::from_str(value)
    } else {
        HeaderValue::from_str(&format!("=?base64?{}?=", STANDARD.encode(value)))
    }
    .map_err(|_| argument_error())?;
    header.set_sensitive(true);
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::integer;
    use serde_json::Number;

    #[test]
    fn header_integers_are_exact_and_javascript_safe() {
        for (raw, expected) in [
            ("42.0", "42"),
            ("-9e3", "-9000"),
            ("9007199254740991", "9007199254740991"),
            ("0.000", "0"),
            ("1.200e2", "120"),
        ] {
            let number: Number = serde_json::from_str(raw).unwrap();
            assert_eq!(integer(&number).unwrap(), expected);
        }
        for raw in [
            "9007199254740991.1",
            "9007199254740992",
            "1e400",
            "1e-400",
            "-9007199254740992",
            "42.00000000000000000001",
        ] {
            let number: Number = serde_json::from_str(raw).unwrap();
            assert!(integer(&number).is_err(), "{raw}");
        }
    }
}
