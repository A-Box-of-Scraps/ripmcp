use super::{Request, target::Target};
use crate::{
    config::{
        Definition, Source,
        schema::{Runtime, Version},
    },
    error::Error,
    ownership::{Installation, InstallationId, Origin, Ownership, ResourceKind, Verification},
};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct Report<'a> {
    schema_version: Version,
    installation: Installed<'a>,
}
#[derive(Serialize)]
struct Installed<'a> {
    id: &'a InstallationId,
    server: &'a str,
    scope: &'a Source,
    target: Destination,
    verification: Verification,
    process_retained: bool,
    prepared: Vec<&'static str>,
    preserved: Vec<Preserved>,
    #[serde(flatten)]
    mutation: crate::config::MutationReport,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Destination {
    Local {
        runtime: Runtime,
        resolution_digest: String,
    },
    Remote {
        endpoint_digest: String,
    },
}
#[derive(Serialize)]
struct Preserved {
    kind: ResourceKind,
    ownership: Ownership,
}

pub(crate) fn report(
    target: &Target,
    installation: &Installation,
    request: &Request,
) -> Result<Value, Error> {
    let destination: Destination = match &installation.origin {
        Origin::Local {
            runtime,
            resolved: Some(resolved),
            ..
        } => Destination::Local {
            runtime: *runtime,
            resolution_digest: crate::config::digest(resolved.as_bytes()),
        },
        Origin::Remote { resource } => Destination::Remote {
            endpoint_digest: crate::config::digest(resource.as_bytes()),
        },
        _ => return Err(super::invalid()),
    };
    let local = matches!(request.server.definition, Definition::Local { .. });
    let report: Report<'_> = Report {
        schema_version: Version,
        installation: Installed {
            id: installation.id(),
            server: &target.identity.name,
            scope: &target.identity.source,
            target: destination,
            verification: installation.verification,
            process_retained: !request.skip_verify && local,
            prepared: if local {
                vec!["runtime_dependencies"]
            } else {
                Vec::new()
            },
            preserved: installation
                .resources
                .iter()
                .map(|resource| Preserved {
                    kind: resource.kind,
                    ownership: resource.ownership,
                })
                .collect(),
            mutation: crate::config::MutationReport {
                shadowed_by_trusted_project: target.shadowed,
                project_reapproval_required: target.project,
            },
        },
    };
    serde_json::to_value(report).map_err(crate::storage::io_error)
}
