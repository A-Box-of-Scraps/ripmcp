use ripmcp::supervisor::Barrier;
use std::time::Duration;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

#[tokio::test]
async fn maintenance_waits_for_operations_and_blocks_new_work() {
    let barrier: Barrier = Barrier::default();
    let active: OwnedRwLockReadGuard<()> = barrier.enter().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), barrier.maintenance())
            .await
            .is_err()
    );
    drop(active);
    let maintenance: OwnedRwLockWriteGuard<()> = barrier.maintenance().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), barrier.enter())
            .await
            .is_err()
    );
    drop(maintenance);
    assert!(barrier.enter().await.is_ok());
}

#[tokio::test]
async fn shutdown_drains_active_work_and_permanently_rejects_new_operations() {
    let barrier: Barrier = Barrier::default();
    let active: OwnedRwLockReadGuard<()> = barrier.enter().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), barrier.close())
            .await
            .is_err()
    );
    assert!(barrier.enter().await.is_err());
    drop(active);
    let closed: OwnedRwLockWriteGuard<()> = barrier.close().await;
    drop(closed);
    assert!(barrier.enter().await.is_err());
    assert!(barrier.maintenance().await.is_err());
}
