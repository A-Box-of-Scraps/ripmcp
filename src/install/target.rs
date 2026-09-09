use crate::{
    config::{Configuration, Effective, Identity, Source},
    error::{Error, ErrorKind},
    mcp::Operation,
    storage::{Location, LockedStore, Paths, Store},
    trust::TrustStore,
};
use std::path::PathBuf;

pub(crate) struct Target {
    pub effective: Effective,
    pub identity: Identity,
    pub bytes: Vec<u8>,
    pub shadowed: bool,
    pub project: bool,
    locked: LockedStore,
    paths: Paths,
    cwd: PathBuf,
    selected: Option<(PathBuf, String)>,
    import_project: Option<super::import::Approval>,
}

impl Target {
    pub async fn lock(
        paths: &Paths,
        cwd: &std::path::Path,
        name: &str,
        request: &super::Request,
        operation: &Operation,
    ) -> Result<Self, Error> {
        if let Some(approval) = &request.import_project {
            approval.check(paths)?;
        }
        super::input::validate(name, &request.server)?;
        let effective: Effective = Effective::load(paths, cwd)?;
        validate_scope(&effective, paths, request.project)?;
        let selected: Option<(PathBuf, String)> = selection(&effective);
        let source: Source = if request.project {
            Source::Project {
                root: effective
                    .project()
                    .ok_or_else(super::invalid)?
                    .root()
                    .to_path_buf(),
            }
        } else {
            Source::User {
                config: paths.directory(Location::Config)?.join("config.json"),
            }
        };
        let (directory, private): (PathBuf, bool) = match &source {
            Source::Project { root } => (root.join(".ripmcp"), false),
            Source::User { config } => (
                config.parent().ok_or_else(super::invalid)?.to_path_buf(),
                false,
            ),
        };
        let locked: LockedStore = Store::new(directory, "config.json", private)?
            .recording(paths)?
            .lock(operation)
            .await?;
        let mut configuration: Configuration = locked
            .previous
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default();
        configuration.register(name.to_owned(), request.server.clone())?;
        configuration.validate()?;
        let bytes: Vec<u8> =
            serde_json::to_vec_pretty(&configuration).map_err(crate::storage::io_error)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(super::invalid());
        }
        let identity: Identity = Identity {
            source,
            name: name.to_owned(),
            configuration_digest: crate::config::digest(&bytes),
        };
        let shadowed = !request.project
            && effective
                .project()
                .is_some_and(|project| project.configuration().servers.contains_key(name))
            && effective
                .project()
                .map(|project| TrustStore::new(paths)?.is_approved(project))
                .transpose()?
                .unwrap_or(false);
        let target: Self = Self {
            effective: Effective::staged(identity.clone(), request.server.clone()),
            identity,
            bytes,
            shadowed,
            project: request.project,
            locked,
            paths: paths.clone(),
            cwd: cwd.to_path_buf(),
            selected,
            import_project: request.import_project.clone(),
        };
        target.check()?;
        Ok(target)
    }

    pub fn check(&self) -> Result<(), Error> {
        if let Some(approval) = &self.import_project {
            approval.check(&self.paths)?;
        }
        self.locked.unchanged()?;
        let effective: Effective = Effective::load(&self.paths, &self.cwd)?;
        if selection(&effective) != self.selected {
            return Err(changed());
        }
        validate_scope(&effective, &self.paths, self.project)
    }

    pub fn commit(&self, operation: &Operation) -> Result<(), Error> {
        self.check()?;
        self.locked.commit(&self.bytes, operation)
    }
}

pub(crate) fn validate_scope(
    effective: &Effective,
    paths: &Paths,
    project: bool,
) -> Result<(), Error> {
    if project {
        let selected: &crate::config::Project = effective.project().ok_or_else(|| {
            Error::new(
                ErrorKind::Configuration,
                "project scope requires a discovered project",
            )
        })?;
        if !TrustStore::new(paths)?.is_approved(selected)? {
            return Err(Error::new(
                ErrorKind::Configuration,
                "trust project before processing installation definitions",
            ));
        }
    }
    Ok(())
}

fn selection(effective: &Effective) -> Option<(PathBuf, String)> {
    effective
        .project()
        .map(|project| (project.root().to_path_buf(), project.digest().to_owned()))
}
fn changed() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "selected project changed during installation; retry after renewing trust",
    )
}
