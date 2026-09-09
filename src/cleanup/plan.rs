use crate::{
    config::{Configuration, Effective, Source},
    error::{Error, ErrorKind},
    ownership::{
        CleanupState, Installation, Journal, Ownership, OwnershipStore, Resource, ResourceIdentity,
        ResourceKind,
    },
    storage::{Location, Paths, Store},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project: bool,
    pub clean: bool,
    pub approval: String,
}

#[derive(Clone, Serialize)]
pub struct Plan {
    pub schema_version: u32,
    pub server: String,
    pub scope: Source,
    pub installation: Option<String>,
    pub configuration_digest: String,
    pub ownership_digest: String,
    pub actions: Vec<&'static str>,
    pub deletable: Vec<ResourceIdentity>,
    pub preserved: Vec<Preserved>,
    pub retry: Vec<String>,
    pub shadowed_by_trusted_project: bool,
    pub project_reapproval_required: bool,
    pub retry_cwd: PathBuf,
}

#[derive(Clone, Serialize)]
pub struct Preserved {
    pub resource: ResourceIdentity,
    pub reason: String,
}

impl Plan {
    pub fn build(paths: &Paths, cwd: &Path, name: &str, request: &Request) -> Result<Self, Error> {
        let effective: Effective = Effective::load(paths, cwd)?;
        let scope: Source = scope(&effective, paths, request.project)?;
        let store: Store = store(&scope)?;
        let bytes: Option<Vec<u8>> = store.read()?;
        let configuration: Configuration = bytes
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default();
        if configuration.servers.contains_key(name) {
            crate::install::target::validate_scope(&effective, paths, request.project)?;
        }
        let journal: Journal = OwnershipStore::new(paths)?.read()?;
        let installation: Option<&Installation> =
            resolve(&journal, &scope, name, configuration.servers.get(name))?;
        if !configuration.servers.contains_key(name) && installation.is_none() {
            return Err(Error::new(
                ErrorKind::Configuration,
                "server is not registered and has no retained cleanup record in selected scope",
            ));
        }
        let mut plan: Self = Self {
            schema_version: 1,
            server: name.to_owned(),
            scope,
            installation: installation.map(|entry| entry.id().as_str().to_owned()),
            configuration_digest: crate::config::digest(bytes.as_deref().unwrap_or_default()),
            ownership_digest: crate::config::digest(
                &serde_json::to_vec(&journal).map_err(crate::storage::io_error)?,
            ),
            actions: vec!["stop_selected_owned_process", "unregister_selected_scope"],
            deletable: Vec::new(),
            preserved: Vec::new(),
            retry: vec![
                "ripmcp".to_owned(),
                "uninstall".to_owned(),
                name.to_owned(),
                if request.project {
                    "--project"
                } else {
                    "--user"
                }
                .to_owned(),
            ],
            shadowed_by_trusted_project: !request.project
                && effective
                    .project()
                    .is_some_and(|project| project.configuration().servers.contains_key(name))
                && effective.authorize(name).is_ok(),
            project_reapproval_required: request.project
                && configuration.servers.contains_key(name),
            retry_cwd: cwd.canonicalize().map_err(crate::storage::io_error)?,
        };
        if request.clean {
            plan.actions.push("delete_tracked_exclusive_resources");
            plan.retry.extend(["--clean".to_owned(), "-y".to_owned()]);
        }
        if let Some(installation) = installation {
            plan.resources(
                &journal,
                installation,
                request.clean,
                &crate::ownership::artifacts::read(paths)?,
            );
        }
        Ok(plan)
    }

    pub fn digest(&self) -> Result<String, Error> {
        Ok(crate::config::digest(
            &serde_json::to_vec(self).map_err(crate::storage::io_error)?,
        ))
    }

    pub(crate) fn resources(
        &mut self,
        journal: &Journal,
        installation: &Installation,
        clean: bool,
        artifacts: &crate::ownership::artifacts::Artifacts,
    ) {
        for resource in &installation.resources {
            if resource.cleanup == CleanupState::Removed {
                continue;
            }
            let reason: Option<&str> = if clean {
                preservation(journal, resource, artifacts)
            } else {
                Some("ordinary uninstall preserves installed data and caches")
            };
            if let Some(reason) = reason {
                self.preserved.push(Preserved {
                    resource: resource.identity.clone(),
                    reason: reason.to_owned(),
                });
            } else {
                self.deletable.push(resource.identity.clone());
            }
        }
        self.deletable.sort_by_key(|identity| {
            std::cmp::Reverse(match identity {
                ResourceIdentity::Path { canonical_path } => canonical_path.components().count(),
                _ => 0,
            })
        });
    }
}

pub(crate) fn preservation<'a>(
    journal: &Journal,
    resource: &'a Resource,
    artifacts: &crate::ownership::artifacts::Artifacts,
) -> Option<&'a str> {
    if let ResourceIdentity::Path { canonical_path } = &resource.identity
        && canonical_path.file_name().is_some_and(|name| {
            matches!(
                name.to_str(),
                Some("ownership.json" | "artifacts.json" | "self-removal.json")
            )
        })
    {
        return Some("cleanup coordination metadata is never server-owned data");
    }
    if artifacts.files.values().any(|artifact| {
        artifact.cleanup != CleanupState::Removed
            && overlaps(&resource.identity, &artifact.identity)
    }) {
        return Some("application artifact is not exclusively owned by this server installation");
    }
    if resource.ownership != Ownership::Exclusive {
        return Some("external, shared or uncertain ownership");
    }
    if !matches!(
        resource.kind,
        ResourceKind::InstallationDirectory
            | ResourceKind::Log
            | ResourceKind::Cache
            | ResourceKind::Data
            | ResourceKind::Symlink
    ) {
        return Some("runtime, executable or setup resource requires separate provenance");
    }
    let ResourceIdentity::Path {
        canonical_path: path,
    }: &ResourceIdentity = &resource.identity
    else {
        return Some("shared runtime resource");
    };
    if path.components().any(|part| part.as_os_str() == ".ripmcp") {
        return Some("project .ripmcp directories are always preserved");
    }
    let Some(proof): &Option<crate::ownership::Filesystem> = &resource.filesystem else {
        return Some("no installation-time filesystem identity; ownership cannot be established");
    };
    if proof.created.is_none() {
        return Some(
            "immutable filesystem birth identity is unavailable; preserve uncertain resource",
        );
    }
    if matches!(resource.kind, ResourceKind::Symlink) && !proof.symlink {
        return Some("recorded symlink identity does not describe a link");
    }
    if path == &proof.root || !path.starts_with(&proof.root) {
        return Some("resource is outside recorded containment boundary");
    }
    let references = journal
        .resources()
        .filter(|other| {
            other.cleanup != CleanupState::Removed && resource.identity == other.identity
        })
        .count();
    let retained = journal.installations.values().any(|entry| {
        entry
            .retained_data
            .iter()
            .any(|other| overlaps(&resource.identity, other))
    });
    if references != 1 || retained || conflicting_reference(journal, resource) {
        return Some("duplicate, overlapping or retained ownership references");
    }
    None
}

pub(crate) fn overlaps(left: &ResourceIdentity, right: &ResourceIdentity) -> bool {
    match (left, right) {
        (
            ResourceIdentity::Path {
                canonical_path: left,
            },
            ResourceIdentity::Path {
                canonical_path: right,
            },
        ) => left.starts_with(right) || right.starts_with(left),
        _ => left == right,
    }
}

fn conflicting_reference(journal: &Journal, resource: &Resource) -> bool {
    journal.installations.values().any(|entry| {
        let owner = entry
            .resources
            .iter()
            .any(|other| std::ptr::eq(other, resource));
        entry.resources.iter().any(|other| {
            other.cleanup != CleanupState::Removed
                && (!owner || other.ownership != Ownership::Exclusive)
                && overlaps(&resource.identity, &other.identity)
        })
    })
}

fn resolve<'a>(
    journal: &'a Journal,
    scope: &Source,
    name: &str,
    server: Option<&crate::config::Server>,
) -> Result<Option<&'a Installation>, Error> {
    let candidates: Vec<&Installation> = journal
        .installations
        .values()
        .filter(|entry| &entry.scope == scope && entry.server == name)
        .filter(|entry| {
            server.is_none_or(|server| {
                entry.registration == crate::ownership::Registration::Registered
                    && entry
                        .definition
                        .as_ref()
                        .is_some_and(|definition| definition.definition == server.definition)
            })
        })
        .collect();
    if server.is_none() && candidates.len() > 1 {
        let pending: Vec<&Installation> = candidates
            .iter()
            .copied()
            .filter(|entry| has_cleanup(journal, entry, true))
            .collect();
        if let [entry] = pending.as_slice() {
            return Ok(Some(*entry));
        }
        let clean: Vec<&Installation> = candidates
            .iter()
            .copied()
            .filter(|entry| has_cleanup(journal, entry, false))
            .collect();
        if let [entry] = clean.as_slice() {
            return Ok(Some(*entry));
        }
    }
    match candidates.as_slice() {
        [] => Ok(None),
        [entry] => Ok(Some(*entry)),
        _ => Err(Error::new(
            ErrorKind::Configuration,
            "ambiguous retained installation records; preserve resources and inspect ownership.json before retrying",
        )),
    }
}

fn has_cleanup(journal: &Journal, entry: &Installation, pending: bool) -> bool {
    journal.operations.values().any(|operation| {
        operation.installation_id == *entry.id()
            && matches!(operation.kind, crate::ownership::OperationKind::Clean)
            && (!pending || !matches!(operation.state, crate::ownership::OperationState::Committed))
    })
}

fn scope(effective: &Effective, paths: &Paths, project: bool) -> Result<Source, Error> {
    if project {
        Ok(Source::Project {
            root: effective
                .project()
                .ok_or_else(super::changed)?
                .root()
                .to_path_buf(),
        })
    } else {
        Ok(Source::User {
            config: paths.directory(Location::Config)?.join("config.json"),
        })
    }
}

pub(crate) fn store(scope: &Source) -> Result<Store, Error> {
    let directory: PathBuf = match scope {
        Source::User { config } => config.parent().ok_or_else(super::changed)?.to_path_buf(),
        Source::Project { root } => root.join(".ripmcp"),
    };
    Store::new(directory, "config.json", false)
}
