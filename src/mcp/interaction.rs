use super::{Operation, protocol_error};
use crate::error::{Error, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::Write;
use tokio::sync::{mpsc, oneshot};
use url::Url;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Prompt {
    pub url: String,
    pub message: String,
}

pub(crate) struct Pending {
    pub prompt: Prompt,
    pub presented: oneshot::Sender<()>,
}

pub(super) enum Interaction {
    Terminal(String),
    Forward(mpsc::Sender<Pending>),
}

impl Prompt {
    fn validate(&self) -> Result<(), Error> {
        let url: Url = Url::parse(&self.url).map_err(|_| protocol_error())?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
        if !self.url.is_ascii()
            || self.url.len() > 8192
            || self.message.len() > 8192
            || self.url.chars().any(char::is_control)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        {
            return Err(protocol_error());
        }
        Ok(())
    }

    pub(crate) fn present(&self, server: &str) -> Result<(), Error> {
        self.validate()?;
        let mut stderr: std::io::StderrLock<'_> = std::io::stderr().lock();
        writeln!(
            stderr,
            "Server {server:?} requests user interaction:\n{}\nOpen this URL to continue: {}\nWaiting for completion; press Ctrl-C to cancel.",
            self.message.escape_debug(),
            self.url
        )
        .and_then(|()| stderr.flush())
        .map_err(crate::storage::io_error)
    }
}

impl Operation {
    pub(crate) fn enable_interaction(&mut self, server: String) {
        self.interaction = Some(Interaction::Terminal(server));
    }

    pub(crate) fn forward_interaction(&mut self, sender: mpsc::Sender<Pending>) {
        self.interaction = Some(Interaction::Forward(sender));
    }

    pub(super) fn interactive(&self) -> bool {
        self.interaction.is_some()
    }

    async fn present(&self, prompt: Prompt) -> Result<(), Error> {
        self.remaining()?;
        match &self.interaction {
            Some(Interaction::Terminal(server)) => prompt.present(server)?,
            Some(Interaction::Forward(sender)) => {
                let (presented, receiver): (oneshot::Sender<()>, oneshot::Receiver<()>) =
                    oneshot::channel();
                self.run(async {
                    sender
                        .send(Pending { prompt, presented })
                        .await
                        .map_err(|_| super::connection_error())?;
                    receiver.await.map_err(|_| super::connection_error())
                })
                .await?;
            }
            None => return Err(unsupported()),
        }
        self.remaining()?;
        Ok(())
    }
}

pub(super) async fn continue_request(
    result: &Value,
    params: &mut Value,
    operation: &Operation,
) -> Result<(), Error> {
    if !operation.interactive() {
        return Err(unsupported());
    }
    if result.get("_meta").is_some_and(|meta| !meta.is_object())
        || result
            .get("requestState")
            .is_some_and(|state| !state.is_string())
    {
        return Err(protocol_error());
    }
    let inputs: &Map<String, Value> = result["inputRequests"]
        .as_object()
        .filter(|inputs| !inputs.is_empty() && inputs.len() <= 16)
        .ok_or_else(unsupported)?;
    let mut prompts: Vec<(&String, Prompt)> = Vec::new();
    for (id, input) in inputs {
        if input["method"] != "elicitation/create" || input["params"]["mode"] != "url" {
            return Err(unsupported());
        }
        let prompt: Prompt = Prompt {
            url: input["params"]["url"]
                .as_str()
                .ok_or_else(protocol_error)?
                .to_owned(),
            message: input["params"]["message"]
                .as_str()
                .ok_or_else(protocol_error)?
                .to_owned(),
        };
        prompt.validate()?;
        prompts.push((id, prompt));
    }
    let mut responses: Map<String, Value> = Map::new();
    for (id, prompt) in prompts {
        operation.present(prompt).await?;
        responses.insert(id.clone(), json!({"action": "accept"}));
    }
    params["inputResponses"] = Value::Object(responses);
    let object: &mut Map<String, Value> = params.as_object_mut().ok_or_else(protocol_error)?;
    object.remove("requestState");
    if let Some(state) = result.get("requestState") {
        object.insert("requestState".to_owned(), state.clone());
    }
    Ok(())
}

fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "tool requires unsupported client input; only URL elicitation with --interactive is supported; invocation was not replayed",
    )
}

#[cfg(test)]
#[path = "interaction/tests.rs"]
mod tests;
