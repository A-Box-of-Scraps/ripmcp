use super::invalid;
use crate::error::{Error, ErrorKind};
use secret_service::{Collection, EncryptionType, Item, SecretService};
use std::{collections::HashMap, future::Future, pin::Pin};

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;

pub trait SecureStore: Send + Sync {
    fn available(&self) -> StoreFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn read<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<Vec<u8>>>;
    fn write<'a>(&'a self, key: &'a str, value: &'a [u8], create: bool) -> StoreFuture<'a, ()>;
}

pub struct SystemStore;

fn unavailable() -> Error {
    Error::new(
        ErrorKind::Authentication,
        "secure Secret Service storage unavailable or locked; unlock the default keyring and retry",
    )
}

async fn service() -> Result<SecretService<'static>, Error> {
    // No autolaunch fallback: headless processes must not start a session bus.
    if !std::env::var("DBUS_SESSION_BUS_ADDRESS")
        .is_ok_and(|address| address.starts_with("unix:") && !address.contains(';'))
    {
        return Err(unavailable());
    }
    SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(|_| unavailable())
}

async fn collection<'a>(service: &'a SecretService<'_>) -> Result<Collection<'a>, Error> {
    let collection: Collection<'_> = service
        .get_default_collection()
        .await
        .map_err(|_| unavailable())?;
    collection
        .ensure_unlocked()
        .await
        .map_err(|_| unavailable())?;
    Ok(collection)
}

fn attributes(key: &str) -> HashMap<&str, &str> {
    HashMap::from([
        ("application", "ripmcp"),
        ("namespace", "oauth-v1"),
        ("identity", key),
    ])
}

async fn item<'a>(collection: &'a Collection<'_>, key: &str) -> Result<Option<Item<'a>>, Error> {
    let mut items: Vec<Item<'_>> = collection
        .search_items(attributes(key))
        .await
        .map_err(|_| unavailable())?;
    if items.len() > 1 {
        return Err(invalid());
    }
    if let Some(item) = items.pop() {
        item.ensure_unlocked().await.map_err(|_| unavailable())?;
        Ok(Some(item))
    } else {
        Ok(None)
    }
}

impl SystemStore {
    pub(crate) async fn reference(id: &str) -> Result<crate::trust::secrets::Secret, Error> {
        let service: SecretService<'_> = service().await?;
        let collection: Collection<'_> = collection(&service).await?;
        let attributes: HashMap<&str, &str> = HashMap::from([
            ("application", "ripmcp"),
            ("namespace", "references-v1"),
            ("identity", id),
        ]);
        let mut items: Vec<Item<'_>> = collection
            .search_items(attributes)
            .await
            .map_err(|_| unavailable())?;
        if items.len() != 1 {
            return Err(unavailable());
        }
        let item: Item<'_> = items.pop().ok_or_else(unavailable)?;
        item.ensure_unlocked().await.map_err(|_| unavailable())?;
        let bytes: Vec<u8> = item.get_secret().await.map_err(|_| unavailable())?;
        if bytes.len() > super::network::BODY_LIMIT {
            return Err(invalid());
        }
        String::from_utf8(bytes)
            .map(crate::trust::secrets::Secret::new)
            .map_err(|_| invalid())
    }
}

impl SecureStore for SystemStore {
    fn available(&self) -> StoreFuture<'_, ()> {
        Box::pin(async {
            let service: SecretService<'_> = service().await?;
            collection(&service).await?;
            Ok(())
        })
    }
    fn read<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let service: SecretService<'_> = service().await?;
            let collection: Collection<'_> = collection(&service).await?;
            let Some(item): Option<Item<'_>> = item(&collection, key).await? else {
                return Ok(None);
            };
            let bytes: Vec<u8> = item.get_secret().await.map_err(|_| unavailable())?;
            if bytes.len() > super::network::BODY_LIMIT {
                return Err(invalid());
            }
            Ok(Some(bytes))
        })
    }

    fn write<'a>(&'a self, key: &'a str, value: &'a [u8], create: bool) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            if value.len() > super::network::BODY_LIMIT {
                return Err(invalid());
            }
            let service: SecretService<'_> = service().await?;
            let collection: Collection<'_> = collection(&service).await?;
            if let Some(item) = item(&collection, key).await? {
                return item
                    .set_secret(value, "application/json")
                    .await
                    .map_err(|_| unavailable());
            }
            if !create {
                return Err(unavailable());
            }
            collection
                .create_item(
                    "ripmcp OAuth credentials",
                    attributes(key),
                    value,
                    false,
                    "application/json",
                )
                .await
                .map_err(|_| unavailable())?;
            Ok(())
        })
    }
}
