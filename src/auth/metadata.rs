use super::{
    challenge::{self, Challenge},
    invalid,
    network::{self, Network},
    unsupported,
};
use crate::{error::Error, mcp::Operation};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

#[derive(Deserialize)]
pub(super) struct Resource {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub bearer_methods_supported: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
pub(super) struct Authorization {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Option<Vec<String>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
    pub client_id_metadata_document_supported: Option<bool>,
    pub authorization_response_iss_parameter_supported: Option<bool>,
}

pub(super) struct Profile {
    pub resource: String,
    pub authorization: Authorization,
    pub scope: Option<String>,
}

impl Profile {
    pub fn add_scope(&mut self, additional: Option<&str>) -> Result<(), Error> {
        self.scope = challenge::union(self.scope.as_deref(), additional)?;
        Ok(())
    }
}

impl Network {
    pub async fn discover(&self, endpoint: &Url, operation: &Operation) -> Result<Profile, Error> {
        let challenge: Challenge = self.probe(endpoint, operation).await?;
        let resource: Resource = self.resource(endpoint, &challenge, operation).await?;
        let expected: String = canonical(endpoint);
        if resource.resource != expected
            || resource.authorization_servers.is_empty()
            || resource.authorization_servers.len() > 16
            || resource
                .bearer_methods_supported
                .as_ref()
                .is_some_and(|methods| !methods.iter().any(|method| method == "header"))
        {
            return Err(invalid());
        }
        // Selection is deterministic; failures never switch to another issuer.
        let issuer: &str = &resource.authorization_servers[0];
        let authorization: Authorization = self.authorization_metadata(issuer, operation).await?;
        let scope: Option<String> = challenge.scope.or_else(|| {
            resource
                .scopes_supported
                .filter(|scopes| !scopes.is_empty())
                .map(|scopes| scopes.join(" "))
        });
        if let Some(scope) = &scope {
            challenge::scopes(scope)?;
        }
        Ok(Profile {
            resource: expected,
            authorization,
            scope,
        })
    }

    async fn probe(&self, endpoint: &Url, operation: &Operation) -> Result<Challenge, Error> {
        let request: Value = json!({"jsonrpc": "2.0", "id": "ripmcp-auth", "method": "server/discover", "params": {
            "_meta": {"io.modelcontextprotocol/protocolVersion": crate::mcp::PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {},
                "io.modelcontextprotocol/clientInfo": {"name": "ripmcp", "version": env!("CARGO_PKG_VERSION")}}
        }});
        let response: reqwest::Response = network::send(
            self.json(endpoint, &request)?
                .header("mcp-protocol-version", crate::mcp::PROTOCOL_VERSION)
                .header("mcp-method", "server/discover"),
            operation,
        )
        .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return challenge::parse(response.headers());
        }
        if !response.status().is_success() && !network::absent(response.status()) {
            return Err(invalid());
        }
        Ok(Challenge::default())
    }

    async fn resource(
        &self,
        endpoint: &Url,
        challenge: &Challenge,
        operation: &Operation,
    ) -> Result<Resource, Error> {
        if let Some(metadata) = &challenge.metadata {
            let url: Url = self.url(metadata)?;
            let response: reqwest::Response = network::send(self.get(&url)?, operation).await?;
            if !response.status().is_success() {
                return Err(invalid());
            }
            return network::document(response, operation).await;
        }
        let candidates: Vec<Url> = well_known(endpoint, "oauth-protected-resource");
        self.first_document(candidates, operation).await
    }

    async fn authorization_metadata(
        &self,
        issuer: &str,
        operation: &Operation,
    ) -> Result<Authorization, Error> {
        let url: Url = self.url(issuer)?;
        if url.query().is_some() {
            return Err(invalid());
        }
        let mut candidates: Vec<Url> = vec![inserted(&url, "oauth-authorization-server")];
        candidates.push(inserted(&url, "openid-configuration"));
        if url.path() != "/" {
            let mut appended: Url = url.clone();
            appended.set_path(&format!(
                "{}/.well-known/openid-configuration",
                url.path().trim_end_matches('/')
            ));
            candidates.push(appended);
        }
        let metadata: Authorization = self.first_document(candidates, operation).await?;
        if metadata.issuer != issuer {
            return Err(invalid());
        }
        self.validate_authorization(&metadata)?;
        Ok(metadata)
    }

    fn validate_authorization(&self, metadata: &Authorization) -> Result<(), Error> {
        self.url(&metadata.authorization_endpoint)?;
        self.url(&metadata.token_endpoint)?;
        if !metadata
            .response_types_supported
            .iter()
            .any(|value| value == "code")
            || !metadata
                .code_challenge_methods_supported
                .as_ref()
                .is_some_and(|values| values.iter().any(|value| value == "S256"))
            || metadata
                .grant_types_supported
                .as_ref()
                .is_some_and(|values| !values.iter().any(|value| value == "authorization_code"))
            || metadata
                .token_endpoint_auth_methods_supported
                .as_ref()
                .is_some_and(|values| !values.iter().any(|value| value == "none"))
        {
            return Err(unsupported());
        }
        Ok(())
    }

    async fn first_document<T: serde::de::DeserializeOwned>(
        &self,
        candidates: Vec<Url>,
        operation: &Operation,
    ) -> Result<T, Error> {
        for candidate in candidates {
            let response: reqwest::Response =
                network::send(self.get(&candidate)?, operation).await?;
            if network::absent(response.status()) {
                continue;
            }
            if !response.status().is_success() {
                return Err(invalid());
            }
            return network::document(response, operation).await;
        }
        Err(unsupported())
    }
}

pub(super) fn canonical(endpoint: &Url) -> String {
    if endpoint.path() == "/" && endpoint.query().is_none() {
        endpoint.as_str().trim_end_matches('/').to_owned()
    } else {
        endpoint.as_str().to_owned()
    }
}

fn inserted(url: &Url, suffix: &str) -> Url {
    let mut result: Url = url.clone();
    result.set_query(None);
    result.set_path(&format!(
        "/.well-known/{suffix}{}",
        url.path().trim_end_matches('/')
    ));
    result
}

fn well_known(endpoint: &Url, suffix: &str) -> Vec<Url> {
    let mut candidates: Vec<Url> = vec![inserted(endpoint, suffix)];
    if endpoint.path() != "/" {
        let mut root: Url = endpoint.clone();
        root.set_query(None);
        root.set_path(&format!("/.well-known/{suffix}"));
        candidates.push(root);
    }
    candidates
}
