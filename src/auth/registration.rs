use super::{Provider, credentials::Credential, metadata::Profile};
use crate::{
    config::authentication::{ClientAuthMethod, OAuthClient},
    error::{Error, ErrorKind},
    mcp::Operation,
    trust::secrets::{Secret, SecretBackend},
};
use reqwest::RequestBuilder;
use url::Url;

pub(super) struct Registration {
    pub configuration: OAuthClient,
    pub secret: Option<Secret>,
}

pub(super) async fn provider(
    endpoint: Url,
    client: Option<&OAuthClient>,
    backend: &(impl SecretBackend + Sync),
    operation: &Operation,
) -> Result<Provider, Error> {
    let mut provider: Provider = configured(endpoint, client)?;
    if let Some(client) = client {
        let secret: Option<Secret> = match &client.client_secret {
            Some(reference) => Some(reference.resolve(backend, operation).await?),
            None => None,
        };
        if secret.as_ref().is_some_and(|value| {
            value.expose().is_empty()
                || value.expose().len() > 16384
                || value.expose().chars().any(char::is_control)
        }) {
            return Err(super::invalid());
        }
        provider
            .network
            .registration
            .as_mut()
            .ok_or_else(super::invalid)?
            .secret = secret;
    }
    Ok(provider)
}

pub(super) fn configured(endpoint: Url, client: Option<&OAuthClient>) -> Result<Provider, Error> {
    let mut provider: Provider = Provider::new(endpoint)?;
    if let Some(client) = client {
        client.validate()?;
        provider.network.url(&client.issuer)?;
        provider.vault.partition = Some(crate::config::digest(
            &serde_json::to_vec(client).map_err(crate::storage::io_error)?,
        ));
        provider.network.registration = Some(Registration {
            configuration: client.clone(),
            secret: None,
        });
    }
    Ok(provider)
}

impl Registration {
    pub fn check_issuer(&self, issuer: &str) -> Result<(), Error> {
        if self.configuration.issuer != issuer {
            return Err(Error::new(
                ErrorKind::Authentication,
                "OAuth issuer differs from the configured client registration; credentials were not sent",
            ));
        }
        Ok(())
    }

    pub fn check_credential(&self, credential: &Credential) -> Result<(), Error> {
        self.check_issuer(&credential.issuer)?;
        if credential.client_id != self.configuration.client_id {
            return Err(super::required());
        }
        Ok(())
    }

    pub fn add_scopes(&self, profile: &mut Profile) -> Result<(), Error> {
        if !self.configuration.scopes.is_empty() {
            profile.add_scope(Some(&self.configuration.scopes.join(" ")))?;
        }
        Ok(())
    }

    pub fn request(
        &self,
        network: &super::network::Network,
        endpoint: &Url,
        fields: &[(&str, &str)],
    ) -> Result<RequestBuilder, Error> {
        let mut fields: Vec<(&str, &str)> = fields.to_vec();
        match self.configuration.token_endpoint_auth_method {
            ClientAuthMethod::None => network.post(endpoint, &fields),
            ClientAuthMethod::ClientSecretPost => {
                fields.push((
                    "client_secret",
                    self.secret.as_ref().ok_or_else(super::invalid)?.expose(),
                ));
                network.post(endpoint, &fields)
            }
            ClientAuthMethod::ClientSecretBasic => {
                fields.retain(|(key, _)| *key != "client_id");
                let secret: &str = self.secret.as_ref().ok_or_else(super::invalid)?.expose();
                Ok(network.post(endpoint, &fields)?.basic_auth(
                    form_encode(&self.configuration.client_id),
                    Some(form_encode(secret)),
                ))
            }
        }
    }
}

fn form_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
