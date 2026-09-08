use crate::{
    config::{Definition, Source},
    error::Error,
    ownership::{
        CleanupState, Installation, InstallationId, Operation, OperationKind, OperationState,
        Origin, Ownership, OwnershipStore, Registration, Resource, ResourceIdentity, ResourceKind,
    },
    storage::Paths,
};

pub(crate) fn begin(
    scope: Source,
    name: String,
    definition: &Definition,
) -> Result<Installation, Error> {
    let origin: Origin = match definition {
        Definition::Local {
            runtime, package, ..
        } => Origin::Local {
            runtime: *runtime,
            requested: package.clone(),
            resolved: None,
            executable: None,
        },
        Definition::Remote { url, .. } => Origin::Remote {
            resource: crate::config::schema::endpoint(url)?.to_string(),
        },
    };
    let mut installation: Installation = Installation::new(scope, name, origin)?;
    installation.registration = Registration::Unregistered;
    if let Definition::Local { runtime, cwd, .. } = definition {
        installation.resources.push(Resource {
            kind: if *runtime == crate::config::schema::Runtime::Docker {
                ResourceKind::Image
            } else {
                ResourceKind::Cache
            },
            identity: ResourceIdentity::Runtime {
                runtime: super::preparation::name(*runtime).to_owned(),
                identity: "shared-runtime-storage".to_owned(),
            },
            origin: Origin::Unknown,
            ownership: Ownership::Shared,
            cleanup: CleanupState::Preserved,
        });
        if let Some(cwd) = cwd
            && cwd != std::path::Path::new("/")
        {
            installation.resources.push(Resource {
                kind: ResourceKind::Data,
                identity: ResourceIdentity::Path {
                    canonical_path: cwd.clone(),
                },
                origin: Origin::Unknown,
                ownership: Ownership::Unknown,
                cleanup: CleanupState::Preserved,
            });
        }
    }
    Ok(installation)
}

pub(crate) async fn save(
    paths: &Paths,
    installation: &Installation,
    state: OperationState,
    operation: &crate::mcp::Operation,
) -> Result<(), Error> {
    let copy: Installation = serde_json::from_slice(
        &serde_json::to_vec(installation).map_err(crate::storage::io_error)?,
    )
    .map_err(crate::storage::io_error)?;
    OwnershipStore::new(paths)?
        .update_async(operation, |journal| {
            let id: InstallationId = installation.id().clone();
            journal.installations.insert(id.as_str().to_owned(), copy);
            journal.operations.insert(
                id.as_str().to_owned(),
                Operation {
                    id: id.clone(),
                    installation_id: id,
                    kind: OperationKind::Install,
                    state,
                    resources: operation_resources(installation)?,
                },
            );
            Ok(())
        })
        .await
}

fn operation_resources(installation: &Installation) -> Result<Vec<ResourceIdentity>, Error> {
    let mut resources: Vec<ResourceIdentity> = installation
        .resources
        .iter()
        .map(|resource| resource.identity.clone())
        .collect();
    if matches!(installation.origin, Origin::Local { .. }) {
        let bytes: Vec<u8> = serde_json::to_vec(&(&installation.scope, &installation.server))
            .map_err(crate::storage::io_error)?;
        for (runtime, identity) in [
            (
                "ripmcp-preparation-lease",
                crate::config::digest(installation.id().as_str().as_bytes()),
            ),
            ("ripmcp-instance-lease", crate::config::digest(&bytes)),
        ] {
            resources.push(ResourceIdentity::Runtime {
                runtime: runtime.to_owned(),
                identity,
            });
        }
    }
    Ok(resources)
}
