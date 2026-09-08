use super::unavailable;
use crate::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Clone, Default)]
pub struct Barrier {
    gate: Arc<RwLock<()>>,
    closing: Arc<AtomicBool>,
}

impl Barrier {
    pub async fn enter(&self) -> Result<OwnedRwLockReadGuard<()>, Error> {
        let guard: OwnedRwLockReadGuard<()> = self.gate.clone().read_owned().await;
        self.check()?;
        Ok(guard)
    }

    pub async fn maintenance(&self) -> Result<OwnedRwLockWriteGuard<()>, Error> {
        let guard: OwnedRwLockWriteGuard<()> = self.gate.clone().write_owned().await;
        self.check()?;
        Ok(guard)
    }

    pub async fn close(&self) -> OwnedRwLockWriteGuard<()> {
        self.closing.store(true, Ordering::Release);
        self.gate.clone().write_owned().await
    }

    fn check(&self) -> Result<(), Error> {
        if self.closing.load(Ordering::Acquire) {
            Err(unavailable())
        } else {
            Ok(())
        }
    }
}
