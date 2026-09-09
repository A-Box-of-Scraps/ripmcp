use crate::{
    error::Error,
    storage::{Location, Paths, Store},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: crate::config::schema::Version,
    keys: BTreeSet<String>,
}

fn store(paths: &Paths) -> Result<Store, Error> {
    Store::new(paths.directory(Location::State)?, "credentials.json", true)?.recording(paths)
}

pub(crate) fn keys(paths: &Paths) -> Result<BTreeSet<String>, Error> {
    let index: Index = store(paths)?
        .read()?
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| super::invalid())?
        .unwrap_or_default();
    if index.keys.iter().any(|key| !valid(key)) {
        return Err(super::invalid());
    }
    Ok(index.keys)
}

pub(super) async fn record(key: &str) -> Result<(), Error> {
    if !valid(key) {
        return Err(super::invalid());
    }
    let operation: crate::mcp::Operation = crate::mcp::Operation::new(
        crate::deadline::Deadline::new(std::time::Duration::from_secs(60)),
        crate::mcp::CancellationToken::new(),
    );
    let locked: crate::storage::LockedStore =
        store(&Paths::from_environment())?.lock(&operation).await?;
    let mut index: Index = locked
        .previous
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| super::invalid())?
        .unwrap_or_default();
    if index.keys.iter().any(|key| !valid(key)) {
        return Err(super::invalid());
    }
    index.keys.insert(key.to_owned());
    locked.commit(
        &serde_json::to_vec_pretty(&index).map_err(crate::storage::io_error)?,
        &operation,
    )
}

fn valid(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(crate) async fn remove(
    store: &dyn super::SecureStore,
    key: &str,
    operation: &crate::mcp::Operation,
) -> Result<(), Error> {
    if !valid(key) {
        return Err(super::invalid());
    }
    operation.run(store.remove(key)).await?;
    if operation.run(store.read(key)).await?.is_some() {
        return Err(super::invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{SecureStore, StoreFuture},
        deadline::Deadline,
        mcp::{CancellationToken, Operation},
    };
    use std::{collections::BTreeMap, sync::Mutex, time::Duration};

    struct Fake(Mutex<BTreeMap<String, Vec<u8>>>);
    impl SecureStore for Fake {
        fn read<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<Vec<u8>>> {
            Box::pin(async move { Ok(self.0.lock().unwrap().get(key).cloned()) })
        }
        fn write<'a>(&'a self, _: &'a str, _: &'a [u8], _: bool) -> StoreFuture<'a, ()> {
            Box::pin(async { unreachable!() })
        }
        fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()> {
            Box::pin(async move {
                self.0.lock().unwrap().remove(key);
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn tracked_credential_removal_is_idempotent_and_preserves_other_keys() {
        let key: String = "a".repeat(64);
        let store: Fake = Fake(Mutex::new(BTreeMap::from([
            (key.clone(), b"test-token".to_vec()),
            ("external".to_owned(), b"keep".to_vec()),
        ])));
        let operation: Operation = Operation::new(
            Deadline::new(Duration::from_secs(1)),
            CancellationToken::new(),
        );
        remove(&store, &key, &operation).await.unwrap();
        remove(&store, &key, &operation).await.unwrap();
        assert!(store.0.lock().unwrap().contains_key("external"));
        assert!(remove(&store, "external", &operation).await.is_err());
    }
}
