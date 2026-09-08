use super::{invalid, metadata::Profile, now, random, required, store::SecureStore};
use crate::{
    error::Error,
    mcp::Operation,
    storage::{Directory, Paths, Store},
};
use rustix::fs::OFlags;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use url::Url;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Credential {
    pub endpoint: String,
    pub resource: String,
    pub issuer: String,
    pub client_id: String,
    pub token_endpoint: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub issued_at: u64,
    pub scope: Option<String>,
}

impl Credential {
    pub fn validate(&self, endpoint: &Url, profile: &Profile) -> Result<(), Error> {
        if self.endpoint != endpoint.as_str()
            || self.resource != profile.resource
            || self.issuer != profile.authorization.issuer
            || self.token_endpoint != profile.authorization.token_endpoint
            || self.client_id.is_empty()
        {
            return Err(required());
        }
        Ok(())
    }

    pub fn expired(&self) -> Result<bool, Error> {
        let current = now()?;
        Ok(current < self.issued_at
            || self
                .expires_at
                .is_some_and(|expiry| expiry <= current.saturating_add(30)))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub schema_version: u32,
    pub generation: String,
    pub credential: Option<Credential>,
    #[serde(default)]
    pub pending_scope: Option<String>,
    #[serde(default)]
    pub login_required: bool,
}

impl Record {
    pub fn empty() -> Result<Self, Error> {
        Ok(Self {
            schema_version: 1,
            generation: random()?,
            credential: None,
            pending_scope: None,
            login_required: false,
        })
    }
}

pub(super) struct Vault {
    pub store: Arc<dyn SecureStore>,
    locks: PathBuf,
}

impl Vault {
    pub fn new(store: Arc<dyn SecureStore>) -> Self {
        Self {
            store,
            locks: PathBuf::from("/tmp"),
        }
    }

    #[cfg(test)]
    pub fn isolated(store: Arc<dyn SecureStore>, locks: PathBuf) -> Self {
        Self { store, locks }
    }

    pub fn key(endpoint: &Url) -> String {
        crate::config::digest(endpoint.as_str().as_bytes())
    }

    fn epoch(&self, endpoint: &Url) -> Result<Store, Error> {
        Store::new(
            self.locks
                .join(format!("ripmcp-{}", rustix::process::geteuid().as_raw())),
            &format!("oauth-{}.epoch", Self::key(endpoint)),
            true,
        )
    }

    fn set_epoch(
        &self,
        endpoint: &Url,
        generation: &str,
        operation: &Operation,
    ) -> Result<(), Error> {
        let deadline: crate::deadline::Deadline =
            crate::deadline::Deadline::new(operation.remaining()?);
        self.epoch(endpoint)?
            .update_with_deadline(&deadline, |_| Ok((generation.as_bytes().to_vec(), ())))?;
        operation.remaining()?;
        Ok(())
    }

    pub async fn lock(&self, endpoint: &Url, operation: &Operation) -> Result<File, Error> {
        operation.run(self.store.available()).await?;
        let directory: Directory = Paths::runtime_fallback(
            Path::new(&self.locks),
            rustix::process::geteuid().as_raw(),
            true,
        )?
        .ok_or_else(invalid)?;
        let file: File = directory
            .file(
                &format!(".oauth-{}.lock", Self::key(endpoint)),
                OFlags::RDWR | OFlags::CREATE,
                true,
            )?
            .ok_or_else(invalid)?;
        operation.run(acquire(file)).await
    }

    pub async fn read(
        &self,
        endpoint: &Url,
        operation: &Operation,
    ) -> Result<Option<Record>, Error> {
        let record: Option<Record> = self.raw(endpoint, operation).await?;
        if let Some(record) = &record
            && self
                .epoch(endpoint)?
                .read()?
                .is_some_and(|epoch| epoch != record.generation.as_bytes())
        {
            return Ok(None);
        }
        Ok(record)
    }

    pub async fn raw(
        &self,
        endpoint: &Url,
        operation: &Operation,
    ) -> Result<Option<Record>, Error> {
        let key: String = Self::key(endpoint);
        let Some(bytes): Option<Vec<u8>> = operation.run(self.store.read(&key)).await? else {
            return Ok(None);
        };
        if bytes.len() > super::network::BODY_LIMIT {
            return Err(invalid());
        }
        let value: serde_json::Value = crate::json::parse(&bytes).map_err(|_| invalid())?;
        let record: Record = serde_json::from_value(value).map_err(|_| invalid())?;
        if record.schema_version != 1
            || record.generation.len() != 43
            || record
                .credential
                .as_ref()
                .is_some_and(|credential| credential.endpoint != endpoint.as_str())
        {
            return Err(invalid());
        }
        Ok(Some(record))
    }

    pub async fn write(
        &self,
        endpoint: &Url,
        record: &Record,
        create: bool,
        operation: &Operation,
    ) -> Result<(), Error> {
        let key: String = Self::key(endpoint);
        let bytes: Vec<u8> = serde_json::to_vec(record).map_err(|_| invalid())?;
        // A cancelled D-Bus write can still finish. An independent epoch keeps it revoked.
        self.set_epoch(endpoint, &random()?, operation)?;
        operation
            .run(self.store.write(&key, &bytes, create))
            .await?;
        let stored: Option<Vec<u8>> = operation.run(self.store.read(&key)).await?;
        if stored.as_deref() != Some(bytes.as_slice()) {
            return Err(invalid());
        }
        self.set_epoch(endpoint, &record.generation, operation)?;
        Ok(())
    }
}

async fn acquire(file: File) -> Result<File, Error> {
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                tokio::time::sleep(Duration::from_millis(5)).await
            }
            Err(_) => return Err(invalid()),
        }
    }
}
