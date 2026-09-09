use super::schema::{Authentication, Definition, SecretReference, endpoint};
use crate::error::Error;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthClient {
    pub issuer: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<SecretReference>,
    #[serde(default)]
    pub token_endpoint_auth_method: ClientAuthMethod,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum ClientAuthMethod {
    #[default]
    None,
    ClientSecretPost,
    ClientSecretBasic,
}

impl ClientAuthMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ClientSecretPost => "client_secret_post",
            Self::ClientSecretBasic => "client_secret_basic",
        }
    }
}

impl OAuthClient {
    pub fn validate(&self) -> Result<(), Error> {
        let issuer: url::Url = endpoint(&self.issuer)?;
        if issuer.scheme() != "https"
            || issuer.query().is_some()
            || self.issuer.len() > 8192
            || self.scopes.len() > 128
            || self.scopes.iter().map(String::len).sum::<usize>() > 8192
            || self.client_id.is_empty()
            || self.client_id.len() > 4096
            || self.client_id.chars().any(char::is_control)
            || (self.token_endpoint_auth_method == ClientAuthMethod::None)
                != self.client_secret.is_none()
            || self.scopes.iter().any(|scope| {
                scope.is_empty()
                    || !scope.bytes().all(|byte| {
                        byte == 0x21
                            || (0x23..=0x5b).contains(&byte)
                            || (0x5d..=0x7e).contains(&byte)
                    })
            })
        {
            return Err(Error::field("servers.definition.oauth_client"));
        }
        if let Some(reference) = &self.client_secret {
            reference.validate()?;
        }
        Ok(())
    }
}

impl SecretReference {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        let value: serde_json::Value =
            serde_json::to_value(self).map_err(crate::storage::io_error)?;
        serde_json::from_value::<Self>(value)
            .map(|_| ())
            .map_err(|_| Error::field("servers.definition.secret_reference"))
    }
}

pub(crate) fn stored_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn validate(definition: &Definition) -> Result<(), Error> {
    let Definition::Remote {
        authentication,
        headers,
        bearer,
        oauth_client,
        ..
    }: &Definition = definition
    else {
        return Ok(());
    };
    if (*authentication == Authentication::Bearer) != bearer.is_some()
        || (oauth_client.is_some() && *authentication != Authentication::Oauth)
        || (*authentication == Authentication::Bearer
            && headers
                .keys()
                .any(|name| name.eq_ignore_ascii_case("authorization")))
    {
        return Err(Error::field("servers.definition.authentication"));
    }
    if let Some(reference) = bearer {
        reference.validate()?;
    }
    if let Some(client) = oauth_client {
        client.validate()?;
    }
    Ok(())
}

impl Definition {
    pub(crate) fn references(&self) -> Vec<&SecretReference> {
        match self {
            Self::Local { env, .. } => env.values().collect(),
            Self::Remote {
                headers,
                bearer,
                oauth_client,
                ..
            } => headers
                .values()
                .chain(bearer.iter())
                .chain(
                    oauth_client
                        .iter()
                        .filter_map(|client| client.client_secret.as_ref()),
                )
                .collect(),
        }
    }
}
