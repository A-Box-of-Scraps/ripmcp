use super::{Client, Operation, headers, limit_error, protocol, protocol_error};
use crate::error::{Error, ErrorKind};
use reqwest::header::HeaderMap;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct Tool(Value);

impl Tool {
    pub fn name(&self) -> &str {
        self.0["name"].as_str().unwrap()
    }
    pub fn input_schema(&self) -> &Value {
        &self.0["inputSchema"]
    }
    pub fn envelope(&self) -> &Value {
        &self.0
    }

    fn parse(value: Value) -> Result<Self, Error> {
        if value["name"].as_str().is_none_or(str::is_empty)
            || !value["inputSchema"].is_object()
            || value["inputSchema"]["type"] != "object"
            || value
                .get("outputSchema")
                .is_some_and(|schema| !schema.is_object())
            || value
                .get("description")
                .is_some_and(|description| !description.is_string())
            || value.get("title").is_some_and(|title| !title.is_string())
            || value.get("_meta").is_some_and(|meta| !meta.is_object())
        {
            return Err(protocol_error());
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Serialize)]
pub struct Discovery {
    pub tools: Vec<Tool>,
    pub rejected_tools: Vec<String>,
}

impl Discovery {
    pub fn require_complete(&self) -> Result<(), Error> {
        if !self.rejected_tools.is_empty() {
            return Err(Error::new(
                ErrorKind::PartialFailure,
                "tool discovery excluded invalid HTTP header annotations; uniqueness cannot be established",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ToolResult(Value);

impl ToolResult {
    pub fn envelope(&self) -> &Value {
        &self.0
    }
    pub fn into_envelope(self) -> Value {
        self.0
    }
    pub fn is_error(&self) -> bool {
        self.0["isError"] == true
    }

    fn parse(value: Value) -> Result<Self, Error> {
        protocol::complete(&value)?;
        let content: &Vec<Value> = value["content"].as_array().ok_or_else(protocol_error)?;
        if value.get("isError").is_some_and(|flag| !flag.is_boolean()) {
            return Err(protocol_error());
        }
        for block in content {
            validate_content(block)?;
        }
        Ok(Self(value))
    }
}

impl Client {
    pub async fn discover_tools(&self, operation: &Operation) -> Result<Discovery, Error> {
        let mut discovery: Discovery = Discovery {
            tools: Vec::new(),
            rejected_tools: Vec::new(),
        };
        let mut cursors: HashSet<String> = HashSet::new();
        let mut names: HashSet<String> = HashSet::new();
        let mut params: Value = json!({});
        let mut bytes: usize = 0;
        for _ in 0..self.limits.pages {
            let page: Value = self
                .request("tools/list", params, HeaderMap::new(), operation)
                .await?;
            protocol::complete(&page)?;
            let size = protocol::encode(&page, self.limits.discovery_bytes)?.len();
            bytes = bytes
                .checked_add(size)
                .filter(|bytes| *bytes <= self.limits.discovery_bytes)
                .ok_or_else(limit_error)?;
            self.append_tools(&page, &mut discovery, &mut names)?;
            operation.remaining()?;
            match page.get("nextCursor") {
                None => return Ok(discovery),
                Some(Value::String(cursor)) if cursors.insert(cursor.clone()) => {
                    params = json!({"cursor": cursor})
                }
                _ => return Err(protocol_error()),
            }
        }
        Err(limit_error())
    }

    fn append_tools(
        &self,
        page: &Value,
        discovery: &mut Discovery,
        names: &mut HashSet<String>,
    ) -> Result<(), Error> {
        let tools: &Vec<Value> = page["tools"].as_array().ok_or_else(protocol_error)?;
        for value in tools {
            if names.len() >= self.limits.tools {
                return Err(limit_error());
            }
            let tool: Tool = Tool::parse(value.clone())?;
            if !names.insert(tool.name().to_owned()) {
                return Err(protocol_error());
            }
            if self.is_http() && headers::annotations(tool.input_schema()).is_err() {
                discovery.rejected_tools.push(tool.name().to_owned());
            } else {
                discovery.tools.push(tool);
            }
        }
        Ok(())
    }

    pub async fn tool(&self, name: &str, operation: &Operation) -> Result<Tool, Error> {
        let discovery: Discovery = self.discover_tools(operation).await?;
        discovery
            .tools
            .into_iter()
            .find(|tool| tool.name() == name)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Usage,
                    "tool was not found or its definition is invalid",
                )
            })
    }

    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        operation: &Operation,
    ) -> Result<ToolResult, Error> {
        if !arguments.is_object() {
            return Err(Error::new(
                ErrorKind::Usage,
                "tool arguments must be a JSON object",
            ));
        }
        let tool: Tool = self.tool(name, operation).await?;
        let headers: HeaderMap = if self.is_http() {
            headers::tool_headers(&tool, &arguments)?
        } else {
            HeaderMap::new()
        };
        let result: Value = self
            .request(
                "tools/call",
                json!({"name": name, "arguments": arguments}),
                headers,
                operation,
            )
            .await?;
        let result: Result<ToolResult, Error> = ToolResult::parse(result);
        operation.remaining()?;
        result
    }
}

fn validate_content(block: &Value) -> Result<(), Error> {
    let valid = match block["type"].as_str() {
        Some("text") => block["text"].is_string(),
        Some("image" | "audio") => block["data"].is_string() && block["mimeType"].is_string(),
        Some("resource_link") => block["uri"].is_string() && block["name"].is_string(),
        Some("resource") => {
            block["resource"]["uri"].is_string()
                && (block["resource"]["text"].is_string() || block["resource"]["blob"].is_string())
        }
        _ => false,
    };
    if valid { Ok(()) } else { Err(protocol_error()) }
}
