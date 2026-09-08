use super::*;
use ripmcp::config::Source;
use ripmcp::ownership::{
    CleanupState, Installation, InstallationId, Journal, Operation, OperationKind, OperationState,
    Origin, Ownership, OwnershipStore, Registration, Resource, ResourceIdentity, ResourceKind,
};

#[test]
fn journal_survives_unregister_and_retains_custom_cleanup_retry_records() {
    let fixture: Fixture = Fixture::new();
    let store: OwnershipStore = OwnershipStore::new(&fixture.paths).unwrap();
    assert!(store.read().unwrap().installations.is_empty());
    assert!(!fixture.root.path().join(".local").exists());
    let custom: PathBuf = fixture.root.path().join("custom-location");
    fs::create_dir(&custom).unwrap();
    let identity: ResourceIdentity = ResourceIdentity::path(&custom).unwrap();
    let mut installation: Installation = Installation::new(
        Source::User {
            config: fixture
                .paths
                .directory(Location::Config)
                .unwrap()
                .join("config.json"),
        },
        "s".to_owned(),
        Origin::Unknown,
    )
    .unwrap();
    let id: InstallationId = installation.id().clone();
    installation.resources.push(Resource {
        kind: ResourceKind::Data,
        identity: identity.clone(),
        origin: Origin::Standalone,
        ownership: Ownership::Exclusive,
        cleanup: CleanupState::RetryRequired,
    });
    installation.retained_data.push(identity.clone());
    let operation: Operation = Operation {
        id: InstallationId::new().unwrap(),
        installation_id: id.clone(),
        kind: OperationKind::Clean,
        state: OperationState::RetryRequired,
        resources: vec![identity],
    };
    store
        .update(|journal| {
            journal
                .installations
                .insert(id.as_str().to_owned(), installation);
            journal
                .operations
                .insert(operation.id.as_str().to_owned(), operation);
            Ok(())
        })
        .unwrap();
    store.update(|journal| journal.unregister(&id)).unwrap();
    fs::remove_dir(&custom).unwrap();
    let journal: Journal = store.read().unwrap();
    let installation: &Installation = &journal.installations[id.as_str()];
    assert_eq!(installation.registration, Registration::Unregistered);
    assert_eq!(
        installation.resources[0].cleanup,
        CleanupState::RetryRequired
    );
    assert_eq!(installation.retained_data.len(), 1);
    assert_eq!(journal.operations.len(), 1);
    assert_eq!(installation.id(), &id);
}

#[test]
fn corrupt_incompatible_and_inconsistent_journals_are_not_replaced() {
    let fixture: Fixture = Fixture::new();
    let store: OwnershipStore = OwnershipStore::new(&fixture.paths).unwrap();
    store.update(|_| Ok(())).unwrap();
    let path: PathBuf = fixture
        .paths
        .directory(Location::State)
        .unwrap()
        .join("ownership.json");
    for bytes in [
        b"partial".as_slice(),
        br#"{"schema_version":2,"installations":{},"operations":{}}"#,
        br#"{"schema_version":1,"installations":{},"operations":{"bad":{}}}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(store.read().is_err());
        assert!(store.update(|_| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn invalid_resource_identity_cannot_be_committed() {
    let fixture: Fixture = Fixture::new();
    let store: OwnershipStore = OwnershipStore::new(&fixture.paths).unwrap();
    let mut installation: Installation = Installation::new(
        Source::Project {
            root: fixture.root.path().to_path_buf(),
        },
        "s".to_owned(),
        Origin::Unknown,
    )
    .unwrap();
    installation.retained_data.push(ResourceIdentity::Path {
        canonical_path: PathBuf::from("/tmp/../"),
    });
    assert!(
        store
            .update(|journal| {
                journal
                    .installations
                    .insert(installation.id().as_str().to_owned(), installation);
                Ok(())
            })
            .is_err()
    );
    assert!(store.read().unwrap().installations.is_empty());
}
