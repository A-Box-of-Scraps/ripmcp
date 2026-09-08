use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value, value::RawValue};
use std::fmt;

pub fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let raw: &RawValue = serde_json::from_slice(bytes)?;
    value(raw.get(), 0)
}

fn value(raw: &str, depth: usize) -> Result<Value, serde_json::Error> {
    if depth >= 128 {
        return Err(serde_json::Error::custom("JSON nesting limit exceeded"));
    }
    let mut parser: serde_json::Deserializer<serde_json::de::StrRead<'_>> =
        serde_json::Deserializer::from_str(raw);
    // Explicit containers avoid interpreting real keys as serde's private number/raw-value tags.
    let value: Value = match raw.as_bytes().first() {
        Some(b'{') => parser.deserialize_map(Container(depth + 1))?,
        Some(b'[') => parser.deserialize_seq(Container(depth + 1))?,
        _ => return serde_json::from_str(raw),
    };
    parser.end()?;
    Ok(value)
}

struct Element(usize);

impl<'de> DeserializeSeed<'de> for Element {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        let raw: &RawValue = <&RawValue>::deserialize(deserializer)?;
        value(raw.get(), self.0).map_err(D::Error::custom)
    }
}

struct Container(usize);

impl<'de> Visitor<'de> for Container {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON container with unique object keys")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values: Map<String, Value> = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate JSON key"));
            }
            values.insert(key, access.next_value_seed(Element(self.0))?);
        }
        Ok(Value::Object(values))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut values: Vec<Value> = Vec::new();
        while let Some(value) = access.next_element_seed(Element(self.0))? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::Value;

    #[test]
    fn preserves_private_looking_keys_and_exact_numbers() {
        let raw: &[u8] = br#" {"extension":{"$serde_json::private::Number":"123","kept":true},"raw":{"$serde_json::private::RawValue":"[1]"},"number":123456789012345678901234567890,"fraction":1.00000000000000000001,"other":[null,"text",false]} "#;
        let value: Value = parse(raw).unwrap();
        assert_eq!(value["extension"]["kept"], true);
        assert!(value["raw"].is_object());
        assert_eq!(
            value["number"].to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(value["fraction"].to_string(), "1.00000000000000000001");
        assert_eq!(parse(&serde_json::to_vec(&value).unwrap()).unwrap(), value);
    }

    #[test]
    fn rejects_duplicate_fields_trailing_documents_and_excessive_depth() {
        for bytes in [
            br#"{"a":1,"a":2}"#.as_slice(),
            br#"[{"a":1,"a":2}]"#,
            b"{} {}",
            b"\xff",
            b"[",
            br#"{"a":"\ud800"}"#,
        ] {
            assert!(parse(bytes).is_err());
        }
        let deep: String = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        assert!(parse(deep.as_bytes()).is_err());
    }

    #[test]
    fn inline_call_arguments_use_the_same_lossless_parser() {
        let call: crate::cli::Call = crate::cli::Call {
            positionals: vec![
                "tool".to_owned(),
                r#"{"$serde_json::private::Number":"123"}"#.to_owned(),
            ],
            input: None,
        };
        let request: crate::call::Request = call.into_request().unwrap();
        let crate::call::Input::Inline(arguments): crate::call::Input = request.input else {
            panic!("expected inline arguments");
        };
        assert_eq!(arguments["$serde_json::private::Number"], "123");
    }
}
