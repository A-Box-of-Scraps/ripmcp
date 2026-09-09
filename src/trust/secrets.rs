use crate::config::{AuthorizedServer, Definition, schema::SecretReference};
use crate::error::{Error, ErrorKind};
use crate::mcp::Operation;
use std::collections::BTreeMap;
use std::fmt;

pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

pub trait SecretBackend {
    fn environment(&self, name: &str) -> Result<Secret, Error>;
    fn keyring(&self, id: &str) -> Result<Secret, Error>;
    fn stored_async<'a>(&'a self, id: &'a str) -> crate::auth::StoreFuture<'a, Secret> {
        Box::pin(crate::auth::SystemStore::stored_reference(id))
    }
    fn keyring_async<'a>(&'a self, id: &'a str) -> crate::auth::StoreFuture<'a, Secret>
    where
        Self: Sync,
    {
        Box::pin(async move { self.keyring(id) })
    }
}

pub struct EnvironmentOnly;
impl SecretBackend for EnvironmentOnly {
    fn environment(&self, name: &str) -> Result<Secret, Error> {
        std::env::var(name).map(Secret::new).map_err(|_| {
            Error::new(
                ErrorKind::Authentication,
                "required environment secret is unavailable",
            )
        })
    }
    fn keyring(&self, _: &str) -> Result<Secret, Error> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "keyring references require asynchronous secure resolution",
        ))
    }
    fn keyring_async<'a>(&'a self, id: &'a str) -> crate::auth::StoreFuture<'a, Secret> {
        Box::pin(crate::auth::SystemStore::reference(id))
    }
}

impl AuthorizedServer<'_> {
    pub async fn resolve_secrets_async(
        &self,
        backend: &(impl SecretBackend + Sync),
        operation: &crate::mcp::Operation,
    ) -> Result<BTreeMap<String, Secret>, Error> {
        self.require_enabled(None)?;
        let references: &BTreeMap<String, SecretReference> = match &self.server().definition {
            Definition::Local { env, .. } => env,
            Definition::Remote { headers, .. } => headers,
        };
        let mut values: BTreeMap<String, Secret> = BTreeMap::new();
        for (name, reference) in references {
            operation.remaining()?;
            let value: Secret = match reference {
                SecretReference::Environment(name) => backend.environment(name)?,
                SecretReference::Keyring(id) => operation.run(backend.keyring_async(id)).await?,
                SecretReference::Stored(id) => operation.run(backend.stored_async(id)).await?,
            };
            validate(&value, &self.server().definition)?;
            values.insert(name.clone(), value);
        }
        Ok(values)
    }

    pub fn resolve_secrets(
        &self,
        backend: &impl SecretBackend,
    ) -> Result<BTreeMap<String, Secret>, Error> {
        self.require_enabled(None)?;
        let references: &BTreeMap<String, SecretReference> = match &self.server().definition {
            Definition::Local { env, .. } => env,
            Definition::Remote { headers, .. } => headers,
        };
        references
            .iter()
            .map(|(name, reference)| {
                let value: Secret = match reference {
                    SecretReference::Environment(name) => backend.environment(name)?,
                    SecretReference::Keyring(id) => backend.keyring(id)?,
                    SecretReference::Stored(_) => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "stored credentials require asynchronous resolution",
                        ));
                    }
                };
                validate(&value, &self.server().definition)?;
                Ok((name.clone(), value))
            })
            .collect()
    }
}

fn validate(value: &Secret, definition: &Definition) -> Result<(), Error> {
    if value.expose().contains('\0')
        || matches!(definition, Definition::Remote { .. })
            && value.expose().chars().any(char::is_control)
    {
        return Err(Error::new(
            ErrorKind::Authentication,
            "secret is invalid for its transport field",
        ));
    }
    Ok(())
}

impl SecretReference {
    pub(crate) async fn resolve(
        &self,
        backend: &(impl SecretBackend + Sync),
        operation: &Operation,
    ) -> Result<Secret, Error> {
        match self {
            Self::Environment(name) => backend.environment(name),
            Self::Keyring(id) => operation.run(backend.keyring_async(id)).await,
            Self::Stored(id) => operation.run(backend.stored_async(id)).await,
        }
    }
}
