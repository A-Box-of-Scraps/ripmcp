use super::{Configuration, Effective, Server, user_store};
use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use crate::storage::{Paths, Store};
use crate::trust::TrustStore;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Copy)]
pub enum WriteScope {
    User,
    Project,
}

#[derive(Serialize)]
pub struct MutationReport {
    pub shadowed_by_trusted_project: bool,
    pub project_reapproval_required: bool,
}

impl Effective {
    pub fn mutate<R>(
        &self,
        paths: &Paths,
        scope: WriteScope,
        name: &str,
        change: impl FnOnce(&mut Configuration) -> Result<R, Error>,
    ) -> Result<(R, MutationReport), Error> {
        self.mutate_with_deadline(
            paths,
            scope,
            name,
            &Deadline::new(Duration::from_secs(self.timeouts().operation_seconds.get())),
            change,
        )
    }

    pub fn mutate_with_deadline<R>(
        &self,
        paths: &Paths,
        scope: WriteScope,
        name: &str,
        deadline: &Deadline,
        change: impl FnOnce(&mut Configuration) -> Result<R, Error>,
    ) -> Result<(R, MutationReport), Error> {
        deadline.remaining()?;
        let current: Option<super::Project> = super::Project::discover(&self.start)?;
        let shadowed = matches!(scope, WriteScope::User)
            && current
                .as_ref()
                .is_some_and(|project| project.configuration().servers.contains_key(name))
            && current
                .as_ref()
                .map(|project| TrustStore::new(paths)?.is_approved(project))
                .transpose()?
                .unwrap_or(false);
        if matches!(scope, WriteScope::Project)
            && current
                .as_ref()
                .map(|project| (project.root(), project.digest()))
                != self
                    .project
                    .as_ref()
                    .map(|project| (project.root(), project.digest()))
        {
            return Err(Error::new(
                ErrorKind::Configuration,
                "selected project changed; reload and renew trust before writing",
            ));
        }
        let store: Store = match scope {
            WriteScope::User => user_store(paths)?,
            WriteScope::Project => {
                let project: &super::Project = self.project.as_ref().ok_or_else(|| {
                    Error::new(
                        ErrorKind::Configuration,
                        "project scope requires a discovered project",
                    )
                })?;
                if !TrustStore::new(paths)?.is_approved(project)? {
                    return Err(Error::new(
                        ErrorKind::Configuration,
                        "trust project before modifying its configuration",
                    ));
                }
                Store::new(project.root().join(".ripmcp"), "config.json", false)?
            }
        };
        let (result, changed): (R, bool) = store.update_with_deadline(deadline, |bytes| {
            self.check_project_bytes(scope, bytes)?;
            let mut config: Configuration = bytes
                .map(Configuration::parse)
                .transpose()?
                .unwrap_or_default();
            let before: Vec<u8> = serde_json::to_vec(&config).map_err(crate::storage::io_error)?;
            let result: R = change(&mut config)?;
            config.validate()?;
            if before == serde_json::to_vec(&config).map_err(crate::storage::io_error)?
                && let Some(bytes) = bytes
            {
                return Ok((bytes.to_vec(), (result, false)));
            }
            let output: Vec<u8> =
                serde_json::to_vec_pretty(&config).map_err(crate::storage::io_error)?;
            let changed = bytes != Some(output.as_slice());
            Ok((output, (result, changed)))
        })?;
        Ok((
            result,
            MutationReport {
                shadowed_by_trusted_project: shadowed,
                project_reapproval_required: matches!(scope, WriteScope::Project) && changed,
            },
        ))
    }

    fn check_project_bytes(&self, scope: WriteScope, bytes: Option<&[u8]>) -> Result<(), Error> {
        if matches!(scope, WriteScope::Project)
            && bytes != self.project.as_ref().map(super::Project::bytes)
        {
            return Err(Error::new(
                ErrorKind::Configuration,
                "project changed; reload and renew trust before writing",
            ));
        }
        Ok(())
    }
}

impl Configuration {
    pub fn register(&mut self, name: String, server: Server) -> Result<(), Error> {
        if self.servers.contains_key(&name) {
            return Err(Error::new(
                ErrorKind::Configuration,
                "server already exists in target scope",
            ));
        }
        self.servers.insert(name, server);
        Ok(())
    }
}

impl From<&crate::cli::Scope> for WriteScope {
    fn from(scope: &crate::cli::Scope) -> Self {
        if scope.project {
            Self::Project
        } else {
            Self::User
        }
    }
}
