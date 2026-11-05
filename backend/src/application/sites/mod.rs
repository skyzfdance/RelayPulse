//! 站点 API 门面：组合站点配置与正式监测查询，保持前端契约。
mod audit;
mod operations;
#[cfg(test)]
#[path = "../../../tests/integration/sites/mod.rs"]
mod tests;
mod types;
pub(crate) use crate::modules::sites::SiteError;
use crate::{
    modules::{
        monitoring::{Monitoring, MonitoringError},
        sites::{Site, SiteService},
    },
    shared::diagnostics::{Diagnostics, Recorder},
};
pub(crate) use audit::SiteBoundaryFailure;
use std::sync::Arc;
pub(crate) use types::{Operation, Reply};
pub(crate) struct Sites {
    sites: Arc<SiteService>,
    monitoring: Arc<Monitoring>,
    diagnostics: Recorder,
    operation_logs: crate::modules::operation_logs::OperationRecorder,
}
impl Sites {
    pub(crate) fn new(
        sites: Arc<SiteService>,
        monitoring: Arc<Monitoring>,
        diagnostics: Arc<Diagnostics>,
    ) -> Self {
        Self {
            sites,
            monitoring,
            diagnostics: Recorder::new(diagnostics),
            operation_logs: Default::default(),
        }
    }
    pub(crate) fn check_ready(&self) -> Result<(), SiteError> {
        self.sites.check_ready()
    }
    pub(crate) fn with_operation_logs(
        mut self,
        logs: Arc<crate::modules::operation_logs::OperationLogs>,
    ) -> Self {
        self.operation_logs = crate::modules::operation_logs::OperationRecorder::new(logs);
        self
    }
    pub(crate) async fn execute(&self, operation: Operation) -> Result<Reply, SiteError> {
        let name = operation.name();
        let mut audit = self.prepare_audit(&operation);
        let result = self
            .diagnostics
            .observe_async(name, self.dispatch(operation, &mut audit))
            .await;
        if let Err(error) = &result {
            audit.failed(&self.operation_logs, error);
        }
        result
    }
    async fn project(&self, site: Site, item: bool) -> Result<Reply, SiteError> {
        let states = self
            .monitoring
            .states(site.models.iter().map(|m| m.id.clone()).collect())
            .await
            .map_err(monitor_error)?;
        Ok(if item {
            Reply::Item(types::SiteListItem::new(&site, &states))
        } else {
            Reply::Site(Box::new(types::ManagedSite::new(site, states)))
        })
    }
    async fn list(&self) -> Result<Reply, SiteError> {
        let sites = self.sites.list().await?;
        let states = self
            .monitoring
            .states(
                sites
                    .iter()
                    .flat_map(|site| site.models.iter().map(|m| m.id.clone()))
                    .collect(),
            )
            .await
            .map_err(monitor_error)?;
        Ok(Reply::List(
            sites
                .iter()
                .map(|site| types::SiteListItem::new(site, &states))
                .collect(),
        ))
    }
}
fn monitor_error(error: MonitoringError) -> SiteError {
    match error {
        MonitoringError::Site(error) => error,
        MonitoringError::Capture => SiteError::Capture,
        MonitoringError::Storage => SiteError::Storage,
        MonitoringError::Worker => SiteError::Worker,
    }
}
