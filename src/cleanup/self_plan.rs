use super::plan::Plan;
use crate::{
    config::{Configuration, Source},
    error::Error,
    ownership::{Journal, artifacts::Artifacts},
    storage::{Location, Paths},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub approval: String,
    pub executable: PathBuf,
}

#[derive(Serialize)]
pub(crate) struct SelfPlan {
    pub schema_version: u32,
    pub configuration_digest: String,
    pub ownership_digest: String,
    pub user_config: PathBuf,
    pub unregister: Vec<String>,
    pub installations: Vec<Plan>,
    pub executable: PathBuf,
    pub executable_action: String,
    pub actions: Vec<&'static str>,
    pub preserved: Vec<Value>,
    pub credentials: Vec<String>,
    pub retry: Vec<&'static str>,
    pub artifacts: Artifacts,
    pub retry_metadata: Vec<PathBuf>,
    pub setup: Vec<crate::ownership::Resource>,
    #[serde(skip)]
    pub endpoints: BTreeSet<String>,
}

impl SelfPlan {
    pub fn build(paths: &Paths, cwd: &Path, executable: &Path) -> Result<Self, Error> {
        let bytes: Option<Vec<u8>> = crate::config::user_store(paths)?.read()?;
        let configuration: Configuration = bytes
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default();
        let journal: Journal = super::self_state::ownership(paths)?;
        let mut artifacts: Artifacts = super::self_state::artifacts(paths)?;
        artifacts
            .files
            .remove(&paths.directory(Location::State)?.join("ownership.json"));
        artifacts.files.remove(
            &paths
                .directory(Location::State)?
                .join(".ownership.json.lock"),
        );
        artifacts
            .files
            .remove(&paths.directory(Location::Config)?.join(".config.json.lock"));
        let mut plan: Self = Self {
            schema_version: 1,
            configuration_digest: crate::config::digest(bytes.as_deref().unwrap_or_default()),
            ownership_digest: crate::config::digest(
                &serde_json::to_vec(&journal).map_err(crate::storage::io_error)?,
            ),
            user_config: paths.directory(Location::Config)?.join("config.json"),
            unregister: configuration.servers.keys().cloned().collect(),
            installations: Vec::new(),
            executable: executable.to_path_buf(),
            executable_action: binary_action(&journal, executable),
            actions: vec![
                "acquire_cross_process_maintenance",
                "stop_all_owned_processes",
                "stop_supervisor",
                "unregister_user_scope",
                "delete_tracked_exclusive_resources",
                "remove_owned_oauth_credentials",
                "remove_standalone_executable_last",
            ],
            preserved: Vec::new(),
            credentials: Vec::new(),
            retry: vec!["ripmcp", "--uninstall-everything", "-y"],
            endpoints: BTreeSet::new(),
            artifacts,
            setup: Vec::new(),
            retry_metadata: [
                "ownership.json",
                "artifacts.json",
                "self-removal.json",
                ".ownership.json.lock",
                ".self-removal.json.lock",
                ".maintenance.lock",
            ]
            .into_iter()
            .map(|name| {
                paths
                    .directory(Location::State)
                    .map(|state| state.join(name))
            })
            .collect::<Result<Vec<PathBuf>, Error>>()?,
        };
        plan.retry_metadata
            .push(paths.directory(Location::Config)?.join(".config.json.lock"));
        plan.populate(paths, cwd, &configuration, &journal)?;
        Ok(plan)
    }

    fn populate(
        &mut self,
        paths: &Paths,
        cwd: &Path,
        configuration: &Configuration,
        journal: &Journal,
    ) -> Result<(), Error> {
        self.artifact_exclusions(journal);
        self.setup_resources(journal);
        for installation in journal.installations.values() {
            self.installation(journal, installation, cwd);
        }
        for server in configuration.servers.values() {
            self.credential(server);
        }
        if let Some(project) = crate::config::Project::discover(cwd)? {
            self.preserve_project(project.root());
            for server in project.configuration().servers.values() {
                self.credential(server);
            }
        }
        for location in [
            Location::Config,
            Location::Data,
            Location::State,
            Location::Cache,
        ] {
            self.preserved.push(json!({"path": paths.directory(location)?, "reason": "untracked entries and parent directories are preserved; no recursive search or deletion"}));
        }
        self.preserved.push(json!({"resource": "external_secret_references_and_unindexed_legacy_credentials", "reason": "only indexed ripmcp OAuth keys and known OAuth endpoints are cleanup targets; external or unproven credentials are preserved"}));
        self.preserved.push(json!({"path": self.executable.parent(), "reason": "executable parent and PATH directories are preserved; only individually recorded files or links are removal targets"}));
        let mut keys: BTreeSet<String> = crate::auth::cleanup::keys(paths)?;
        for endpoint in &self.endpoints {
            keys.insert(crate::config::digest(
                crate::config::schema::endpoint(endpoint)?
                    .as_str()
                    .as_bytes(),
            ));
        }
        self.credentials = keys.into_iter().collect();
        Ok(())
    }

    fn installation(
        &mut self,
        journal: &Journal,
        installation: &crate::ownership::Installation,
        cwd: &Path,
    ) {
        let mut plan: Plan = Plan {
            schema_version: 1,
            server: installation.server.clone(),
            scope: installation.scope.clone(),
            installation: Some(installation.id().as_str().to_owned()),
            configuration_digest: self.configuration_digest.clone(),
            ownership_digest: self.ownership_digest.clone(),
            actions: vec!["stop_owned_process", "delete_tracked_exclusive_resources"],
            deletable: Vec::new(),
            preserved: Vec::new(),
            retry: self
                .retry
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
            shadowed_by_trusted_project: false,
            project_reapproval_required: false,
            retry_cwd: cwd.to_path_buf(),
        };
        plan.resources(journal, installation, true, &self.artifacts);
        self.installations.push(plan);
        if let Source::Project { root } = &installation.scope {
            self.preserve_project(root);
        }
        if let Source::User { config } = &installation.scope
            && config != &self.user_config
            && !self.artifacts.files.contains_key(config)
        {
            self.preserved.push(json!({"path": config, "reason": "recorded custom configuration has no installation-time artifact identity; preserved"}));
        }
        if let Some(server) = &installation.definition {
            self.credential(server);
        }
    }

    fn preserve_project(&mut self, root: &Path) {
        let value: Value = json!({"path": root.join(".ripmcp"), "reason": "project configuration is preserved; local definitions may no longer run after managed data and installation records are removed"});
        if !self.preserved.contains(&value) {
            self.preserved.push(value);
        }
    }

    fn credential(&mut self, server: &crate::config::Server) {
        if let crate::config::Definition::Remote {
            url,
            authentication: crate::config::schema::Authentication::Oauth,
            ..
        } = &server.definition
        {
            self.endpoints.insert(url.clone());
        }
    }

    fn artifact_exclusions(&mut self, journal: &Journal) {
        for (path, artifact) in &mut self.artifacts.files {
            if journal
                .installations
                .values()
                .flat_map(|entry| &entry.resources)
                .any(|resource| {
                    resource.cleanup != crate::ownership::CleanupState::Removed
                        && super::plan::overlaps(&resource.identity, &artifact.identity)
                })
            {
                artifact.cleanup = crate::ownership::CleanupState::Preserved;
                self.preserved.push(json!({"path": path, "reason": "duplicate or overlapping application/server ownership references"}));
            }
        }
    }

    fn setup_resources(&mut self, journal: &Journal) {
        if let crate::ownership::Executable::Standalone { setup, .. } = &journal.executable {
            for link in setup
                .iter()
                .filter(|link| link.cleanup != crate::ownership::CleanupState::Removed)
            {
                match super::plan::preservation(journal, link, &self.artifacts) {
                    Some(reason) => self
                        .preserved
                        .push(json!({"resource": link.identity, "reason": reason})),
                    None => self.setup.push(link.clone()),
                }
            }
        }
    }

    pub fn digest(&self) -> Result<String, Error> {
        Ok(crate::config::digest(
            &serde_json::to_vec(self).map_err(crate::storage::io_error)?,
        ))
    }
}

fn binary_action(journal: &Journal, executable: &Path) -> String {
    match &journal.executable {
        crate::ownership::Executable::Standalone { resource, .. } if resource.identity == (crate::ownership::ResourceIdentity::Path { canonical_path: executable.to_path_buf() }) => "remove_recorded_standalone_executable_last".to_owned(),
        crate::ownership::Executable::PackageManager { manager } => format!("preserve binary; use {manager}'s supported uninstall operation; removal remains incomplete"),
        _ => "preserve binary; installation provenance is unknown or does not match the running executable; manual removal is required".to_owned(),
    }
}
