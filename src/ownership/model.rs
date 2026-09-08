use super::invalid;
use crate::config::Source;
use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct InstallationId(String);
impl InstallationId {
    pub fn new() -> Result<Self, Error> {
        let mut bytes: [u8; 16] = [0; 16];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(crate::storage::io_error)?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for InstallationId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            Ok(Self(value))
        } else {
            Err("invalid installation ID")
        }
    }
}
impl From<InstallationId> for String {
    fn from(id: InstallationId) -> Self {
        id.0
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    id: InstallationId,
    pub scope: Source,
    pub server: String,
    pub origin: Origin,
    pub registration: Registration,
    pub resources: Vec<Resource>,
    pub retained_data: Vec<ResourceIdentity>,
}
impl Installation {
    pub fn new(scope: Source, server: String, origin: Origin) -> Result<Self, Error> {
        Ok(Self {
            id: InstallationId::new()?,
            scope,
            server,
            origin,
            registration: Registration::Registered,
            resources: Vec::new(),
            retained_data: Vec::new(),
        })
    }
    pub fn id(&self) -> &InstallationId {
        &self.id
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.server.is_empty() {
            return Err(invalid());
        }
        let scope_path: &Path = match &self.scope {
            Source::User { config } => config,
            Source::Project { root } => root,
        };
        ResourceIdentity::Path {
            canonical_path: scope_path.to_path_buf(),
        }
        .validate()?;
        for resource in &self.resources {
            resource.identity.validate()?;
        }
        for reference in &self.retained_data {
            reference.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Origin {
    Local {
        runtime: crate::config::schema::Runtime,
        requested: String,
        resolved: Option<String>,
    },
    Remote {
        resource: String,
    },
    Standalone,
    PackageManager {
        manager: String,
    },
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    Registered,
    Unregistered,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Exclusive,
    Shared,
    Unknown,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    InstallationDirectory,
    Log,
    Cache,
    Data,
    Container,
    Image,
    Volume,
    Executable,
    Symlink,
    ShellSetup,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceIdentity {
    Path { canonical_path: PathBuf },
    Runtime { runtime: String, identity: String },
}
impl ResourceIdentity {
    pub fn path(path: &Path) -> Result<Self, Error> {
        Ok(Self::Path {
            canonical_path: path.canonicalize().map_err(crate::storage::io_error)?,
        })
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Path { canonical_path }
                if canonical_path.is_absolute()
                    && canonical_path != Path::new("/")
                    && canonical_path
                        .components()
                        .all(|part| matches!(part, Component::RootDir | Component::Normal(_))) =>
            {
                Ok(())
            }
            Self::Runtime { runtime, identity } if !runtime.is_empty() && !identity.is_empty() => {
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub kind: ResourceKind,
    pub identity: ResourceIdentity,
    pub origin: Origin,
    pub ownership: Ownership,
    pub cleanup: CleanupState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupState {
    Pending,
    Removed,
    Preserved,
    RetryRequired,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: InstallationId,
    pub installation_id: InstallationId,
    pub kind: OperationKind,
    pub state: OperationState,
    pub resources: Vec<ResourceIdentity>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Install,
    Unregister,
    Clean,
    SelfUninstall,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Prepared,
    Applying,
    Committed,
    RetryRequired,
}
