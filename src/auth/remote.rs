use super::{Provider, command::recheck};
use crate::{
    config::{AuthorizedServer, Definition, Effective, Identity, schema::Authentication},
    error::{Error, ErrorKind},
    mcp::{
        AuthenticationProvider, AuthorizationFuture, ChallengeFuture, Client, Discovery,
        HttpOptions, Limits, Operation, Tool, ToolResult,
    },
    storage::Paths,
    supervisor::Action,
    trust::secrets::{EnvironmentOnly, Secret},
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use url::Url;

struct Configured {
    provider: Option<Provider>,
    authorization: Option<HeaderValue>,
    identity: Identity,
    cwd: PathBuf,
    paths: Paths,
}

impl AuthenticationProvider for Configured {
    fn authorization<'a>(
        &'a self,
        resource: &'a Url,
        operation: &'a Operation,
    ) -> AuthorizationFuture<'a> {
        Box::pin(async move {
            recheck(&self.paths, &self.cwd, &self.identity)?;
            let result: Option<HeaderValue> = match &self.provider {
                Some(provider) => provider.authorization(resource, operation).await?,
                None => self.authorization.clone(),
            };
            recheck(&self.paths, &self.cwd, &self.identity)?;
            Ok(result)
        })
    }

    fn challenged_with_authorization<'a>(
        &'a self,
        resource: &'a Url,
        status: reqwest::StatusCode,
        headers: &'a HeaderMap,
        authorization: Option<&'a HeaderValue>,
        operation: &'a Operation,
    ) -> ChallengeFuture<'a> {
        Box::pin(async move {
            recheck(&self.paths, &self.cwd, &self.identity)?;
            match &self.provider {
                Some(provider) => {
                    provider
                        .challenged_with_authorization(
                            resource,
                            status,
                            headers,
                            authorization,
                            operation,
                        )
                        .await
                }
                None => Err(super::required()),
            }
        })
    }
}

pub(crate) async fn execute(
    cwd: PathBuf,
    name: &str,
    action: &Action,
    operation: &Operation,
) -> Result<Value, Error> {
    if matches!(action, Action::Start | Action::Stop) {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "remote servers have no local lifecycle",
        ));
    }
    let paths: Paths = Paths::from_environment();
    let effective: Effective = Effective::load(&paths, &cwd)?;
    let server: AuthorizedServer<'_> = effective.authorize(name)?;
    server.require_enabled(tool_name(action))?;
    let options: HttpOptions = options(&server, &cwd, operation).await?;
    let client: Client = Client::http(options, Limits::default(), operation).await?;
    let result: Result<Value, Error> = invoke(&client, &server, action, operation).await;
    client.shutdown().await;
    result
}

pub(crate) async fn options(
    server: &AuthorizedServer<'_>,
    cwd: &std::path::Path,
    operation: &Operation,
) -> Result<HttpOptions, Error> {
    server.require_enabled(None)?;
    let Definition::Remote {
        url,
        authentication,
        ..
    }: &Definition = &server.server().definition
    else {
        return Err(super::invalid());
    };
    let endpoint: Url = crate::config::schema::endpoint(url)?;
    let secrets: BTreeMap<String, Secret> = server
        .resolve_secrets_async(&EnvironmentOnly, operation)
        .await?;
    let mut headers: HeaderMap = HeaderMap::new();
    let mut authorization: Option<HeaderValue> = None;
    for (name, secret) in secrets {
        let name: HeaderName =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| super::invalid())?;
        let mut value: HeaderValue =
            HeaderValue::from_str(secret.expose()).map_err(|_| super::invalid())?;
        value.set_sensitive(true);
        if name == "authorization" {
            if authorization.replace(value).is_some() {
                return Err(super::invalid());
            }
        } else if headers.insert(name, value).is_some() {
            return Err(super::invalid());
        }
    }
    let provider: Option<Provider> = match authentication {
        Authentication::None => None,
        Authentication::Oauth if authorization.is_none() => Some(Provider::new(endpoint.clone())?),
        _ => {
            return Err(Error::new(
                ErrorKind::Configuration,
                "OAuth cannot be combined with configured Authorization",
            ));
        }
    };
    Ok(HttpOptions {
        endpoint,
        headers,
        authentication: Arc::new(Configured {
            provider,
            authorization,
            identity: server.identity().clone(),
            cwd: cwd.to_path_buf(),
            paths: Paths::from_environment(),
        }),
    })
}

async fn invoke(
    client: &Client,
    server: &AuthorizedServer<'_>,
    action: &Action,
    operation: &Operation,
) -> Result<Value, Error> {
    match action {
        Action::Tools { all } => {
            let mut discovery: Discovery = client.discover_tools(operation).await?;
            discovery.require_complete()?;
            if !all {
                discovery
                    .tools
                    .retain(|tool| server.require_enabled(Some(tool.name())).is_ok());
            }
            Ok(json!({"schema_version": 1, "tools": discovery.tools}))
        }
        Action::Tool { tool } => {
            let tool: Tool = client.tool(tool, operation).await?;
            Ok(json!({"schema_version": 1, "tool": tool}))
        }
        Action::Call { tool, arguments } => {
            let arguments: Value =
                crate::json::parse(arguments.as_bytes()).map_err(|_| super::invalid())?;
            let tool: Tool = client.tool(tool, operation).await?;
            let result: ToolResult = client.call_tool(&tool, arguments, operation).await?;
            Ok(result.into_envelope())
        }
        _ => Err(super::invalid()),
    }
}

fn tool_name(action: &Action) -> Option<&str> {
    match action {
        Action::Tool { tool } | Action::Call { tool, .. } => Some(tool),
        _ => None,
    }
}
