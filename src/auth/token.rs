use super::{
    CLIENT_ID, REDIRECT_URI,
    credentials::Credential,
    invalid,
    metadata::Profile,
    network::{self, Network},
    now, required, unsupported,
};
use crate::{error::Error, mcp::Operation};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

#[derive(Deserialize)]
struct Registration {
    client_id: String,
    client_secret: Option<String>,
    token_endpoint_auth_method: String,
    redirect_uris: Vec<String>,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl Network {
    pub async fn register(
        &self,
        profile: &Profile,
        previous: Option<&Credential>,
        operation: &Operation,
    ) -> Result<String, Error> {
        if profile.authorization.client_id_metadata_document_supported == Some(true) {
            return Ok(CLIENT_ID.to_owned());
        }
        if let Some(previous) = previous
            && previous.issuer == profile.authorization.issuer
            && previous.client_id != CLIENT_ID
        {
            return Ok(previous.client_id.clone());
        }
        let endpoint: Url = self.url(
            profile
                .authorization
                .registration_endpoint
                .as_deref()
                .ok_or_else(unsupported)?,
        )?;
        let request: Value = json!({"client_name": "ripmcp", "redirect_uris": [REDIRECT_URI],
            "application_type": "native", "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"], "token_endpoint_auth_method": "none"});
        let response: reqwest::Response =
            network::send(self.json(&endpoint, &request)?, operation).await?;
        if !response.status().is_success() {
            return Err(unsupported());
        }
        let registration: Registration = network::document(response, operation).await?;
        if registration.client_id.is_empty()
            || registration.client_id.len() > 4096
            || registration.client_id.chars().any(char::is_control)
            || registration.client_secret.is_some()
            || registration.token_endpoint_auth_method != "none"
            || registration.redirect_uris != [REDIRECT_URI]
        {
            return Err(unsupported());
        }
        Ok(registration.client_id)
    }

    pub async fn exchange(
        &self,
        profile: &Profile,
        fields: &[(&str, &str)],
        operation: &Operation,
    ) -> Result<Credential, Error> {
        let endpoint: Url = self.url(&profile.authorization.token_endpoint)?;
        let response: reqwest::Response =
            network::send(self.post(&endpoint, fields)?, operation).await?;
        if !response.status().is_success() {
            return Err(required());
        }
        let token: Token = network::document(response, operation).await?;
        credential(token, profile)
    }
}

fn credential(token: Token, profile: &Profile) -> Result<Credential, Error> {
    if token.expires_in == Some(0)
        || !token.token_type.eq_ignore_ascii_case("bearer")
        || !bearer(&token.access_token)
        || token.refresh_token.as_ref().is_some_and(|token| {
            token.is_empty() || token.len() > 16384 || token.chars().any(char::is_control)
        })
    {
        return Err(invalid());
    }
    if let Some(scope) = &token.scope {
        super::challenge::scopes(scope)?;
    }
    let issued_at = now()?;
    let expires_at: Option<u64> = token
        .expires_in
        .map(|seconds| issued_at.checked_add(seconds).ok_or_else(invalid))
        .transpose()?;
    Ok(Credential {
        endpoint: String::new(),
        resource: profile.resource.clone(),
        issuer: profile.authorization.issuer.clone(),
        client_id: String::new(),
        token_endpoint: profile.authorization.token_endpoint.clone(),
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at,
        issued_at,
        scope: token.scope.or_else(|| profile.scope.clone()),
    })
}

pub(super) fn bearer(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16384
        && value
            .trim_end_matches('=')
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte))
        && !value.trim_end_matches('=').is_empty()
}
