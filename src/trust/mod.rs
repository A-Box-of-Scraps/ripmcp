mod preview;
pub mod secrets;

use crate::config::{Effective, Project, schema::Version};
use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use crate::storage::{Location, Paths, Store};
pub use preview::Preview;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Approvals {
    schema_version: Version,
    #[serde(deserialize_with = "crate::config::unique::map")]
    approvals: BTreeMap<PathBuf, String>,
}

impl Approvals {
    fn validate(&self) -> Result<(), Error> {
        for (root, digest) in &self.approvals {
            if !root.is_absolute()
                || root
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                || digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(corrupt());
            }
        }
        Ok(())
    }
}

pub struct TrustStore {
    store: Store,
    directory: PathBuf,
}

impl TrustStore {
    pub fn new(paths: &Paths) -> Result<Self, Error> {
        let directory: PathBuf = paths.directory(Location::State)?;
        Ok(Self {
            store: Store::new(directory.clone(), "trust.json", true)?,
            directory,
        })
    }

    pub fn is_approved(&self, project: &Project) -> Result<bool, Error> {
        self.require_external(project)?;
        let Some(bytes): Option<Vec<u8>> = self.store.read()? else {
            return Ok(false);
        };
        let approvals: Approvals = serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
        approvals.validate()?;
        Ok(approvals
            .approvals
            .get(project.root())
            .is_some_and(|digest| digest == project.digest()))
    }

    fn require_external(&self, project: &Project) -> Result<(), Error> {
        if self.directory.starts_with(project.root()) {
            return Err(Error::new(
                ErrorKind::Configuration,
                "trust state must be outside the project; set XDG_STATE_HOME to an external private location",
            ));
        }
        Ok(())
    }

    pub fn approve(&self, project: &Project) -> Result<(), Error> {
        self.approve_with_deadline(project, &Deadline::new(Duration::from_secs(60)))
    }

    pub fn approve_with_deadline(
        &self,
        project: &Project,
        deadline: &Deadline,
    ) -> Result<(), Error> {
        self.require_external(project)?;
        self.store
            .update_json_with_deadline::<Approvals, _>(deadline, |approvals| {
                approvals.validate()?;
                approvals
                    .approvals
                    .insert(project.root().to_path_buf(), project.digest().to_owned());
                Ok(())
            })
    }
}

fn corrupt() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "trust state is corrupt or incompatible; approval was not accepted",
    )
}

pub fn run(paths: &Paths, start: &Path, timeout: Option<u64>) -> Result<(), Error> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(Error::new(
            ErrorKind::Configuration,
            "trust requires a terminal; run trust interactively",
        ));
    }
    let effective: Effective = Effective::load(paths, start)?;
    let project: &Project = effective
        .project()
        .ok_or_else(|| Error::new(ErrorKind::Configuration, "no project configuration found"))?;
    let mut stderr: std::io::StderrLock<'static> = std::io::stderr().lock();
    crate::output::json(&mut stderr, &Preview::new(project)?)?;
    stderr
        .write_all(b"Approve these exact configuration bytes and canonical project root? [y/N] ")
        .map_err(crate::storage::io_error)?;
    stderr.flush().map_err(crate::storage::io_error)?;
    let mut answer: String = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(crate::storage::io_error)?;
    if !matches!(answer.trim(), "y" | "Y") {
        return Err(Error::new(
            ErrorKind::Configuration,
            "project trust was not approved",
        ));
    }
    let seconds = timeout.unwrap_or(effective.timeouts().operation_seconds.get());
    TrustStore::new(paths)?
        .approve_with_deadline(project, &Deadline::new(Duration::from_secs(seconds)))?;
    crate::output::json(
        &mut std::io::stdout().lock(),
        &TrustReport {
            schema_version: 1,
            trust: ApprovalReport {
                root: project.root(),
                configuration_digest: project.digest(),
                approved: true,
            },
        },
    )
}

#[derive(Serialize)]
struct TrustReport<'a> {
    schema_version: u32,
    trust: ApprovalReport<'a>,
}
#[derive(Serialize)]
struct ApprovalReport<'a> {
    root: &'a Path,
    configuration_digest: &'a str,
    approved: bool,
}
