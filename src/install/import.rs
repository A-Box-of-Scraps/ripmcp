use crate::{
    config::Project,
    error::{Error, ErrorKind},
    storage::Paths,
    trust::TrustStore,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    root: PathBuf,
    digest: String,
}
impl Approval {
    pub(crate) fn check(&self, paths: &Paths) -> Result<(), Error> {
        let project: Project = Project::discover(&self.root)?.ok_or_else(required)?;
        if project.root() != self.root
            || project.digest() != self.digest
            || !TrustStore::new(paths)?.is_approved(&project)?
        {
            return Err(required());
        }
        Ok(())
    }
}

pub(super) fn authorize(file: Option<&Path>, paths: &Paths) -> Result<Option<Approval>, Error> {
    let Some(file): Option<&Path> = file else {
        return Ok(None);
    };
    let file: PathBuf = file.canonicalize().map_err(crate::storage::io_error)?;
    let parent: &Path = file.parent().ok_or_else(required)?;
    let Some(project): Option<Project> = Project::discover(parent)? else {
        return Ok(None);
    };
    let approval: Approval = Approval {
        root: project.root().to_path_buf(),
        digest: project.digest().to_owned(),
    };
    approval.check(paths)?;
    Ok(Some(approval))
}
fn required() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "imported project definition requires unchanged project trust; run trust in that project before importing",
    )
}
