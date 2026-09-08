use super::{Action, LocalRequest, Manager, Prepared, State, action, changed, key, stop};
use crate::config::{AuthorizedServer, Effective};
use crate::error::{Error, ErrorKind};
use crate::mcp::{Client, Discovery, Limits, Operation, Tool, ToolResult};
use serde_json::{Value, json};
use std::time::Instant;

impl Manager {
    pub(super) async fn execute(
        &self,
        state: &mut State,
        prepared: Prepared,
        request: &LocalRequest,
        lease_key: &str,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let reused = state
            .client
            .as_ref()
            .is_some_and(|client| !client.is_closed())
            && state.revision == prepared.revision
            && state.installation.as_ref() == Some(&prepared.installation);
        if !reused {
            let replacing = state.client.is_some();
            stop(state).await;
            if replacing {
                let effective: Effective = self.effective(request)?;
                self.require_clean(&key(effective.identity(&request.server)?)?)?;
            }
            let lease: std::fs::File = self.lease(lease_key, operation).await?;
            self.recheck(request, &prepared.revision, operation).await?;
            operation.remaining()?;
            // The daemon lifetime lock and slot mutex serialize the guard handoff.
            drop(lease);
            state.configuration_digest = prepared.configuration_digest.clone();
            let limits: Limits = Limits {
                in_flight: 16,
                ..Limits::default()
            };
            let result: Result<Client, Error> =
                Client::stdio(prepared.options, limits, operation).await;
            state.failed = result.is_err();
            let client: Client = result?;
            if let Err(error) = self.recheck(request, &prepared.revision, operation).await {
                client.shutdown().await;
                return Err(error);
            }
            state.client = Some(client);
            state.revision = prepared.revision;
            state.configuration_digest = prepared.configuration_digest;
            state.installation = Some(prepared.installation);
            state.observed = Some(Instant::now());
        }
        let client: &Client = state
            .client
            .as_ref()
            .ok_or_else(super::super::unavailable)?;
        let result: Result<Value, Error> = match &request.action {
            Action::Start => Ok(action(&request.server, "start", reused)),
            _ => {
                self.invoke(client, &state.revision, request, operation)
                    .await
            }
        };
        state.failed = result
            .as_ref()
            .is_err_and(|error| matches!(error.kind, ErrorKind::Connection | ErrorKind::Protocol));
        if result.is_ok() && !matches!(request.action, Action::Start) {
            state.observed = Some(Instant::now());
        }
        result
    }

    async fn invoke(
        &self,
        client: &Client,
        revision: &str,
        request: &LocalRequest,
        operation: &Operation,
    ) -> Result<Value, Error> {
        match &request.action {
            Action::Tools { all } => {
                let discovery: Discovery = client.discover_tools(operation).await?;
                self.recheck(request, revision, operation).await?;
                let effective: Effective = self.effective(request)?;
                let server: AuthorizedServer<'_> = effective.authorize(&request.server)?;
                Ok(crate::tools::report(discovery, &server, *all))
            }
            Action::Tool { tool } => {
                let tool: Tool = client.tool(tool, operation).await?;
                self.recheck(request, revision, operation).await?;
                Ok(json!({"schema_version": 1, "tool": tool}))
            }
            Action::Call { tool, arguments } => {
                let arguments: Value = crate::json::parse(arguments.as_bytes())
                    .map_err(|_| Error::new(ErrorKind::Usage, "invalid tool arguments"))?;
                let tool: Tool = client.tool(tool, operation).await?;
                self.recheck(request, revision, operation).await?;
                let result: ToolResult = client.call_tool(&tool, arguments, operation).await?;
                Ok(result.into_envelope())
            }
            _ => Err(super::super::incompatible()),
        }
    }

    async fn recheck(
        &self,
        request: &LocalRequest,
        revision: &str,
        operation: &Operation,
    ) -> Result<(), Error> {
        let effective: Effective = self.effective(request)?;
        let server: AuthorizedServer<'_> = effective.authorize(&request.server)?;
        let key: String = key(server.identity())?;
        let prepared: Prepared =
            Prepared::load(&server, &self.paths, &request.environment, &key, operation).await?;
        let current: Effective = self.effective(request)?;
        if current.identity(&request.server)? != server.identity() || prepared.revision != revision
        {
            return Err(changed());
        }
        Ok(())
    }
}
