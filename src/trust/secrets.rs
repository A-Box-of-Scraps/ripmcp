use crate::config::{AuthorizedServer, Definition, schema::SecretReference};
use crate::error::{Error, ErrorKind};
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
            "secure keyring backend is not implemented yet",
        ))
    }
}

impl AuthorizedServer<'_> {
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
                };
                if value.expose().contains('\0')
                    || matches!(&self.server().definition, Definition::Remote { .. })
                        && value.expose().chars().any(char::is_control)
                {
                    return Err(Error::new(
                        ErrorKind::Authentication,
                        "secret is invalid for its transport field",
                    ));
                }
                Ok((name.clone(), value))
            })
            .collect()
    }
}
