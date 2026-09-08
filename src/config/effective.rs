use super::{Configuration, Project, Server, digest, user_store};
use crate::deadline::Timeouts;
use crate::error::{Error, ErrorKind};
use crate::storage::{Location, Paths};
use crate::trust::TrustStore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    User { config: PathBuf },
    Project { root: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub source: Source,
    pub name: String,
    pub configuration_digest: String,
}

struct Entry {
    identity: Identity,
    server: Server,
    trust_required: bool,
}

pub struct Effective {
    entries: BTreeMap<String, Entry>,
    timeouts: Timeouts,
    pub(super) project: Option<Project>,
    pub(super) start: PathBuf,
}

#[derive(Serialize)]
pub struct ServerReport<'a> {
    pub name: &'a str,
    pub provenance: &'a Source,
    pub enabled: bool,
    pub trust_required: bool,
}

pub struct AuthorizedServer<'a> {
    entry: &'a Entry,
}

impl Effective {
    pub(crate) fn staged(identity: Identity, server: Server) -> Self {
        Self {
            entries: BTreeMap::from([(
                identity.name.clone(),
                Entry {
                    identity,
                    server,
                    trust_required: false,
                },
            )]),
            timeouts: Timeouts::default(),
            project: None,
            start: PathBuf::from("/"),
        }
    }
    pub fn identity(&self, name: &str) -> Result<&Identity, Error> {
        self.entries
            .get(name)
            .map(|entry| &entry.identity)
            .ok_or_else(|| Error::new(ErrorKind::Configuration, "server is not configured"))
    }

    pub fn is_local(&self, name: &str) -> bool {
        self.entries
            .get(name)
            .is_some_and(|entry| matches!(entry.server.definition, super::Definition::Local { .. }))
    }

    pub fn load(paths: &Paths, start: &Path) -> Result<Self, Error> {
        let bytes: Option<Vec<u8>> = user_store(paths)?.read()?;
        let user: Configuration = bytes
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default();
        let project: Option<Project> = Project::discover(start)?;
        let trusted = match &project {
            Some(project) => TrustStore::new(paths)?.is_approved(project)?,
            None => false,
        };
        let source: Source = Source::User {
            config: canonical_user_path(paths, bytes.is_some())?,
        };
        let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
        insert(
            &mut entries,
            &user,
            source,
            &digest(bytes.as_deref().unwrap_or_default()),
            false,
        );
        let mut timeouts: Timeouts = user.timeouts.unwrap_or_default();
        if let Some(project) = &project {
            insert(
                &mut entries,
                project.configuration(),
                Source::Project {
                    root: project.root().to_path_buf(),
                },
                project.digest(),
                !trusted,
            );
            if trusted && let Some(settings) = &project.configuration().timeouts {
                timeouts = settings.clone();
            }
        }
        Ok(Self {
            entries,
            timeouts,
            project,
            start: start.canonicalize().map_err(crate::storage::io_error)?,
        })
    }

    pub fn reports(&self) -> Vec<ServerReport<'_>> {
        self.entries
            .iter()
            .map(|(name, entry)| ServerReport {
                name,
                provenance: &entry.identity.source,
                enabled: entry.server.enabled,
                trust_required: entry.trust_required,
            })
            .collect()
    }

    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    pub fn timeouts(&self) -> &Timeouts {
        &self.timeouts
    }

    pub fn authorize(&self, name: &str) -> Result<AuthorizedServer<'_>, Error> {
        let entry: &Entry = self
            .entries
            .get(name)
            .ok_or_else(|| Error::new(ErrorKind::Configuration, "server is not configured"))?;
        if entry.trust_required {
            return Err(Error::new(
                ErrorKind::Configuration,
                "project trust required; run trust in a terminal",
            ));
        }
        Ok(AuthorizedServer { entry })
    }
}

impl AuthorizedServer<'_> {
    pub fn identity(&self) -> &Identity {
        &self.entry.identity
    }
    pub fn server(&self) -> &Server {
        &self.entry.server
    }
    pub fn require_enabled(&self, tool: Option<&str>) -> Result<(), Error> {
        if !self.entry.server.enabled {
            return Err(Error::new(ErrorKind::Configuration, "server is disabled"));
        }
        if tool.is_some_and(|tool| self.entry.server.disabled_tools.contains(tool)) {
            return Err(Error::new(ErrorKind::Configuration, "tool is disabled"));
        }
        Ok(())
    }
}

fn insert(
    entries: &mut BTreeMap<String, Entry>,
    config: &Configuration,
    source: Source,
    digest: &str,
    trust_required: bool,
) {
    for (name, server) in &config.servers {
        entries.insert(
            name.clone(),
            Entry {
                identity: Identity {
                    source: source.clone(),
                    name: name.clone(),
                    configuration_digest: digest.to_owned(),
                },
                server: server.clone(),
                trust_required,
            },
        );
    }
}

fn canonical_user_path(paths: &Paths, exists: bool) -> Result<PathBuf, Error> {
    let path: PathBuf = paths.directory(Location::Config)?.join("config.json");
    if exists {
        path.canonicalize().map_err(crate::storage::io_error)
    } else {
        Ok(path)
    }
}
