use crate::cli::Call;
use crate::error::{Error, ErrorKind};
use serde_json::{Map, Value};
use std::path::PathBuf;

pub enum Input {
    Inline(Map<String, Value>),
    File(PathBuf),
    Stdin,
}

pub struct Request {
    pub server: Option<String>,
    pub tool: String,
    pub input: Input,
}

impl Call {
    pub fn into_request(self) -> Result<Request, Error> {
        let mut names: Vec<String> = self.positionals;
        let input: Input = match self.input {
            Some(path) if path.as_os_str() == "-" => Input::Stdin,
            Some(path) => Input::File(path),
            None => {
                let raw: String = names.pop().ok_or_else(invalid)?;
                let value: Value = crate::json::parse(raw.as_bytes()).map_err(|_| invalid())?;
                match value {
                    Value::Object(object) => Input::Inline(object),
                    _ => return Err(invalid()),
                }
            }
        };
        if !(1..=2).contains(&names.len()) || names.iter().any(String::is_empty) {
            return Err(invalid());
        }
        let tool: String = names.pop().ok_or_else(invalid)?;
        Ok(Request {
            server: names.pop(),
            tool,
            input,
        })
    }
}

fn invalid() -> Error {
    Error::new(
        ErrorKind::Usage,
        "expected call [server] tool JSON-object or call [server] tool --input file|-",
    )
}
