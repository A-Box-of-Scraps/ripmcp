use super::{
    callback::{Browser, Callback},
    credentials::{Credential, Record, Vault},
    invalid,
    metadata::Profile,
    network::Network,
    random, required,
    store::{SecureStore, SystemStore},
};
use crate::{
    error::Error,
    mcp::{AuthenticationProvider, AuthorizationFuture, ChallengeFuture, Operation},
};
use reqwest::header::{HeaderMap, HeaderValue};
use std::{fs::File, sync::Arc};
use url::Url;

pub struct Provider {
    pub(super) endpoint: Url,
    pub(super) network: Network,
    pub(super) vault: Vault,
}

impl Provider {
    pub fn new(endpoint: Url) -> Result<Self, Error> {
        Self::with_store(endpoint, Arc::new(SystemStore))
    }

    pub fn with_store(endpoint: Url, store: Arc<dyn SecureStore>) -> Result<Self, Error> {
        let network: Network = Network::new()?;
        if endpoint.scheme() != "https" || endpoint.query().is_some() {
            return Err(crate::error::Error::new(
                crate::error::ErrorKind::Unsupported,
                "OAuth requires a query-free HTTPS MCP endpoint",
            ));
        }
        network.url(endpoint.as_str())?;
        Ok(Self {
            endpoint,
            network,
            vault: Vault::new(store),
        })
    }

    pub(super) async fn login(
        &self,
        browser: &impl Browser,
        recheck: impl Fn() -> Result<(), Error>,
        operation: &Operation,
    ) -> Result<(), Error> {
        operation.run(self.vault.store.available()).await?;
        let callback: Callback = operation.run(Callback::bind()).await?;
        self.login_with(callback, browser, recheck, operation).await
    }

    pub(super) async fn login_with(
        &self,
        callback: Callback,
        browser: &impl Browser,
        recheck: impl Fn() -> Result<(), Error>,
        operation: &Operation,
    ) -> Result<(), Error> {
        let record: Record = self.begin(operation).await?;
        let mut profile: Profile = self.network.discover(&self.endpoint, operation).await?;
        profile.add_scope(record.pending_scope.as_deref())?;
        if let Some(previous) = &record.credential
            && previous.issuer == profile.authorization.issuer
            && previous.resource == profile.resource
        {
            profile.add_scope(previous.scope.as_deref())?;
        }
        let client_id: String = self
            .network
            .register(&profile, record.credential.as_ref(), operation)
            .await?;
        let url: Url = callback.authorization_url(&profile, &client_id)?;
        recheck()?;
        operation.run(browser.open(&url, operation)).await?;
        let code: String = callback.receive(&profile, operation).await?;
        recheck()?;
        let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
        self.require_generation(&record.generation, operation)
            .await?;
        let fields: [(&str, &str); 6] = [
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", &callback.verifier),
            ("redirect_uri", &callback.redirect),
            ("client_id", &client_id),
            ("resource", &profile.resource),
        ];
        let mut credential: Credential =
            self.network.exchange(&profile, &fields, operation).await?;
        credential.endpoint = self.endpoint.as_str().to_owned();
        credential.client_id = client_id;
        recheck()?;
        let record: Record = Record {
            schema_version: 1,
            generation: random()?,
            credential: Some(credential),
            pending_scope: None,
            login_required: false,
        };
        self.vault
            .write(&self.endpoint, &record, false, operation)
            .await
    }

    async fn begin(&self, operation: &Operation) -> Result<Record, Error> {
        let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
        let mut record: Record = self
            .vault
            .read(&self.endpoint, operation)
            .await?
            .unwrap_or(Record::empty()?);
        record.generation = random()?;
        self.vault
            .write(&self.endpoint, &record, true, operation)
            .await?;
        Ok(record)
    }

    async fn require_generation(
        &self,
        generation: &str,
        operation: &Operation,
    ) -> Result<(), Error> {
        if self
            .vault
            .read(&self.endpoint, operation)
            .await?
            .is_none_or(|record| record.generation != generation)
        {
            return Err(crate::error::Error::new(
                crate::error::ErrorKind::Authentication,
                "credentials changed during login; start an explicit login again",
            ));
        }
        Ok(())
    }

    pub async fn status(&self, operation: &Operation) -> Result<&'static str, Error> {
        let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
        let record: Option<Record> = self.vault.read(&self.endpoint, operation).await?;
        if record.as_ref().is_some_and(|record| record.login_required) {
            return Ok("login_required");
        }
        let credential: Option<Credential> = record.and_then(|record| record.credential);
        match credential {
            None => Ok("signed_out"),
            Some(credential) if credential.expired()? => Ok("expired"),
            Some(_) => Ok("saved"),
        }
    }

    pub async fn logout(&self, operation: &Operation) -> Result<(), Error> {
        let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
        if self.vault.raw(&self.endpoint, operation).await?.is_some() {
            self.vault
                .write(&self.endpoint, &Record::empty()?, false, operation)
                .await?;
        }
        Ok(())
    }

    async fn credential(&self, operation: &Operation) -> Result<Credential, Error> {
        let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
        let record: Record = self
            .vault
            .read(&self.endpoint, operation)
            .await?
            .ok_or_else(required)?;
        if record.login_required {
            return Err(required());
        }
        let credential: Credential = record.credential.ok_or_else(required)?;
        let profile: Profile = self.network.discover(&self.endpoint, operation).await?;
        credential.validate(&self.endpoint, &profile)?;
        if !credential.expired()? {
            return Ok(credential);
        }
        self.refresh(credential, &profile, operation).await
    }

    async fn refresh(
        &self,
        previous: Credential,
        profile: &Profile,
        operation: &Operation,
    ) -> Result<Credential, Error> {
        let refresh: &str = previous.refresh_token.as_deref().ok_or_else(required)?;
        // Persist invalidation before rotating: a crash/timeout cannot replay a consumed refresh token.
        self.vault
            .write(&self.endpoint, &Record::empty()?, false, operation)
            .await?;
        let fields: [(&str, &str); 4] = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &previous.client_id),
            ("resource", &profile.resource),
        ];
        let refresh_profile: Profile = Profile {
            resource: profile.resource.clone(),
            authorization: profile.authorization.clone(),
            scope: previous.scope.clone(),
        };
        let mut credential: Credential = self
            .network
            .exchange(&refresh_profile, &fields, operation)
            .await?;
        if credential
            .refresh_token
            .as_deref()
            .is_some_and(|token| token == refresh)
        {
            return Err(invalid());
        }
        credential.endpoint = previous.endpoint;
        credential.client_id = previous.client_id;
        credential.scope = credential.scope.or(previous.scope);
        let record: Record = Record {
            schema_version: 1,
            generation: random()?,
            credential: Some(credential.clone()),
            pending_scope: None,
            login_required: false,
        };
        self.vault
            .write(&self.endpoint, &record, false, operation)
            .await?;
        Ok(credential)
    }
}

impl AuthenticationProvider for Provider {
    fn authorization<'a>(
        &'a self,
        resource: &'a Url,
        operation: &'a Operation,
    ) -> AuthorizationFuture<'a> {
        Box::pin(async move {
            if resource != &self.endpoint {
                return Err(invalid());
            }
            let credential: Credential = self.credential(operation).await?;
            if !super::token::bearer(&credential.access_token) {
                return Err(invalid());
            }
            let mut header: HeaderValue =
                HeaderValue::from_str(&format!("Bearer {}", credential.access_token))
                    .map_err(|_| invalid())?;
            header.set_sensitive(true);
            Ok(Some(header))
        })
    }

    fn challenged<'a>(
        &'a self,
        resource: &'a Url,
        _: reqwest::StatusCode,
        _: &'a HeaderMap,
        _: &'a Operation,
    ) -> ChallengeFuture<'a> {
        Box::pin(async move {
            if resource != &self.endpoint {
                return Err(invalid());
            }
            Err(required())
        })
    }

    fn challenged_with_authorization<'a>(
        &'a self,
        resource: &'a Url,
        _: reqwest::StatusCode,
        headers: &'a HeaderMap,
        authorization: Option<&'a HeaderValue>,
        operation: &'a Operation,
    ) -> ChallengeFuture<'a> {
        Box::pin(async move {
            if resource != &self.endpoint {
                return Err(invalid());
            }
            let challenge: super::challenge::Challenge = super::challenge::parse(headers)?;
            let _lock: File = self.vault.lock(&self.endpoint, operation).await?;
            let Some(mut record): Option<Record> =
                self.vault.read(&self.endpoint, operation).await?
            else {
                return Err(required());
            };
            let Some(credential): Option<&Credential> = record.credential.as_ref() else {
                return Err(required());
            };
            let expected: String = format!("Bearer {}", credential.access_token);
            if authorization.and_then(|value| value.to_str().ok()) != Some(expected.as_str()) {
                return Err(required());
            }
            if let Some(scope) = challenge.scope {
                super::challenge::scopes(&scope)?;
                record.pending_scope =
                    super::challenge::union(record.pending_scope.as_deref(), Some(&scope))?;
            }
            record.login_required = true;
            record.generation = random()?;
            self.vault
                .write(&self.endpoint, &record, false, operation)
                .await?;
            Err(required())
        })
    }
}
