use super::{Action, LocalRequest, Manager, Slot, State, key, stop};
use crate::{
    cleanup::{self_plan::SelfPlan, self_remove::SelfReport},
    config::Identity,
    error::Error,
    mcp::Operation,
    ownership::{InstallationId, OperationKind, OperationState, OwnershipStore},
    storage::Location,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{MutexGuard, OwnedRwLockWriteGuard};

impl Manager {
    pub(super) async fn self_uninstall(
        &self,
        request: LocalRequest,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let Action::SelfUninstall(options): &Action = &request.action else {
            return Err(crate::cleanup::changed());
        };
        self.require_maintenance()?;
        let _barrier: OwnedRwLockWriteGuard<()> = operation.run(self.barrier.maintenance()).await?;
        let plan: SelfPlan = SelfPlan::build(&self.paths, &request.cwd, &options.executable)?;
        if plan.digest()? != options.approval {
            return Err(crate::cleanup::changed());
        }
        self.begin_removal(&plan, operation).await?;
        let mut report: SelfReport = SelfReport {
            schema_version: 1,
            plan,
            completed: Vec::new(),
            failures: Vec::new(),
            installations: Vec::new(),
            configuration_after: None,
        };
        if let Err(error) = self.stop_for_removal(&report.plan).await {
            report.failure("stop_owned_processes", &error);
        }
        serde_json::to_value(report).map_err(crate::storage::io_error)
    }

    async fn begin_removal(&self, plan: &SelfPlan, operation: &Operation) -> Result<(), Error> {
        OwnershipStore::new(&self.paths)?
            .update_async(operation, |journal| {
                if crate::config::digest(
                    &serde_json::to_vec(&journal).map_err(crate::storage::io_error)?,
                ) != plan.ownership_digest
                {
                    return Err(crate::cleanup::changed());
                }
                for plan in &plan.installations {
                    let id: InstallationId = journal.installations[plan
                        .installation
                        .as_ref()
                        .ok_or_else(crate::cleanup::changed)?]
                    .id()
                    .clone();
                    journal.operations.insert(
                        id.as_str().to_owned(),
                        crate::ownership::Operation {
                            id: id.clone(),
                            installation_id: id,
                            kind: OperationKind::SelfUninstall,
                            state: OperationState::Applying,
                            resources: plan.deletable.clone(),
                        },
                    );
                }
                Ok(())
            })
            .await
    }

    async fn stop_for_removal(&self, plan: &SelfPlan) -> Result<(), Error> {
        let slots: MutexGuard<'_, std::collections::BTreeMap<String, Arc<Slot>>> =
            self.slots.lock().await;
        for (key, slot) in slots.iter() {
            let mut state: MutexGuard<'_, State> = slot.state.lock().await;
            stop(&mut state).await;
            self.require_clean(key)?;
        }
        for plan in &plan.installations {
            let identity: Identity = Identity {
                source: plan.scope.clone(),
                name: plan.server.clone(),
                configuration_digest: plan.configuration_digest.clone(),
            };
            self.require_clean(&key(&identity)?)?;
            if let Some(id) = &plan.installation {
                self.require_clean(&crate::config::digest(id.as_bytes()))?;
            }
        }
        Ok(())
    }

    fn require_maintenance(&self) -> Result<(), Error> {
        let directory: crate::storage::Directory =
            crate::storage::Directory::open(&self.paths.directory(Location::State)?, false, true)?
                .ok_or_else(crate::cleanup::changed)?;
        let file: std::fs::File = directory
            .file(".maintenance.lock", rustix::fs::OFlags::RDWR, true)?
            .ok_or_else(crate::cleanup::changed)?;
        if !matches!(
            file.try_lock_shared(),
            Err(std::fs::TryLockError::WouldBlock)
        ) || directory.read("self-removal.json", true)?.is_none()
        {
            return Err(crate::cleanup::changed());
        }
        Ok(())
    }
}
