use super::{Directory, invalid};
use crate::error::Error;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy)]
pub enum Location {
    Config,
    Data,
    State,
    Cache,
}

pub struct Paths {
    environment: BTreeMap<OsString, OsString>,
}

impl Paths {
    pub(crate) fn environment(&self) -> &BTreeMap<OsString, OsString> {
        &self.environment
    }

    pub fn from_environment() -> Self {
        Self {
            environment: [
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR",
            ]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
            .collect(),
        }
    }
    pub fn new(environment: BTreeMap<OsString, OsString>) -> Self {
        Self { environment }
    }

    pub fn directory(&self, location: Location) -> Result<PathBuf, Error> {
        let (key, default): (&str, &str) = match location {
            Location::Config => ("XDG_CONFIG_HOME", ".config"),
            Location::Data => ("XDG_DATA_HOME", ".local/share"),
            Location::State => ("XDG_STATE_HOME", ".local/state"),
            Location::Cache => ("XDG_CACHE_HOME", ".cache"),
        };
        if let Some(path) = self.absolute(key) {
            return Ok(path.join("ripmcp"));
        }
        let home: PathBuf = self.absolute("HOME").ok_or_else(invalid)?;
        Ok(home.join(default).join("ripmcp"))
    }

    pub fn runtime(&self, create: bool) -> Result<Option<Directory>, Error> {
        self.runtime_at(Path::new("/tmp"), create)
    }

    pub fn runtime_at(
        &self,
        fallback_base: &Path,
        create: bool,
    ) -> Result<Option<Directory>, Error> {
        if let Some(base) = self.absolute("XDG_RUNTIME_DIR")
            && matches!(Directory::open(&base, false, true), Ok(Some(_)))
        {
            return Directory::open(&base.join("ripmcp"), create, true);
        }
        Self::runtime_fallback(fallback_base, rustix::process::geteuid().as_raw(), create)
    }

    pub fn runtime_fallback(
        base: &Path,
        uid: u32,
        create: bool,
    ) -> Result<Option<Directory>, Error> {
        if uid != rustix::process::geteuid().as_raw() {
            return Err(invalid());
        }
        Directory::open(&base.join(format!("ripmcp-{uid}")), create, true)
    }

    fn absolute(&self, key: &str) -> Option<PathBuf> {
        let path: PathBuf = PathBuf::from(self.environment.get(OsStr::new(key))?);
        (path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, Component::ParentDir)))
        .then_some(path)
    }
}
