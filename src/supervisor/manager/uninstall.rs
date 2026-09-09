use super::{Action, LocalRequest, Manager, Slot, State, key, stop};
use crate::{
    cleanup::{
        Report, journal,
        plan::{self, Plan},
    },
    config::{Configuration, Identity},
    error::Error,
    mcp::Operation,
    storage::LockedStore,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{MutexGuard, OwnedRwLockWriteGuard};

impl Manager {
    pub(super) async fn uninstall(
        &self,
        request: LocalRequest,
        operation: &Operation,
    ) -> Result<Value, Error> {
        let Action::Uninstall(options): &Action = &request.action else {
            return Err(crate::cleanup::changed());
        };
        let _barrier: OwnedRwLockWriteGuard<()> = operation.run(self.barrier.maintenance()).await?;
        let plan: Plan = Plan::build(&self.paths, &request.cwd, &request.server, options)?;
        if plan.digest()? != options.approval {
            return Err(crate::cleanup::changed());
        }
        let locked: LockedStore = plan::store(&plan.scope)?
            .recording(&self.paths)?
            .lock(operation)
            .await?;
        if crate::config::digest(locked.previous.as_deref().unwrap_or_default())
            != plan.configuration_digest
        {
            return Err(crate::cleanup::changed());
        }
        journal::begin(&self.paths, &plan, options.clean, operation).await?;
        let mut report: Report = Report {
            schema_version: 1,
            completed: Vec::new(),
            failures: Vec::new(),
            plan,
        };
        let result: Result<(), Error> = self.apply_uninstall(&locked, &mut report, operation).await;
        if let Err(error) = result {
            report.failure("stop_or_unregister", None, &error);
        }
        if report.failures.is_empty() && options.clean {
            let plan: Plan = report.plan.clone();
            let result: Result<(), Error> =
                journal::delete(&self.paths, &plan, operation, &mut report).await;
            if let Err(error) = result {
                report.failure("cleanup_journal", None, &error);
            }
        }
        journal::finish(&self.paths, &report.plan, operation, &report).await?;
        serde_json::to_value(report).map_err(crate::storage::io_error)
    }

    async fn apply_uninstall(
        &self,
        locked: &LockedStore,
        report: &mut Report,
        operation: &Operation,
    ) -> Result<(), Error> {
        let identity: Identity = Identity {
            source: report.plan.scope.clone(),
            name: report.plan.server.clone(),
            configuration_digest: report.plan.configuration_digest.clone(),
        };
        let key: String = key(&identity)?;
        let slot: Arc<Slot> = self.slot(&key).await?;
        let mut state: MutexGuard<'_, State> = slot.state.lock().await;
        locked.unchanged()?;
        stop(&mut state).await;
        self.require_clean(&key)?;
        if let Some(id) = &report.plan.installation {
            self.require_clean(&crate::config::digest(id.as_bytes()))?;
        }
        report
            .completed
            .push(json!({"action": "stop", "server": report.plan.server}));
        let mut configuration: Configuration = locked
            .previous
            .as_deref()
            .map(Configuration::parse)
            .transpose()?
            .unwrap_or_default();
        if configuration.servers.remove(&report.plan.server).is_some() {
            let bytes: Vec<u8> =
                serde_json::to_vec_pretty(&configuration).map_err(crate::storage::io_error)?;
            locked.commit(&bytes, operation)?;
        }
        journal::unregistered(&self.paths, &report.plan, operation).await?;
        report
            .completed
            .push(json!({"action": "unregister", "server": report.plan.server}));
        Ok(())
    }
}
