//! 站点 API 门面：组合站点配置与正式监测查询，保持前端契约。
mod operations;
#[cfg(test)]
#[path = "../../../tests/integration/sites/mod.rs"]
mod tests;
mod types;
pub(crate) use crate::modules::sites::SiteError;
use crate::{
    modules::{
        monitoring::{Monitoring, MonitoringError},
        sites::{Site, SiteService, TaskError},
    },
    shared::diagnostics::{Diagnostics, Recorder},
};
use std::sync::Arc;
pub(crate) use types::{Operation, Reply};
pub(crate) struct Sites {
    sites: Arc<SiteService>,
    monitoring: Arc<Monitoring>,
    diagnostics: Recorder,
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
        }
    }
    pub(crate) fn check_ready(&self) -> Result<(), SiteError> {
        self.sites.check_ready()
    }
    pub(crate) async fn execute(&self, operation: Operation) -> Result<Reply, SiteError> {
        self.diagnostics
            .observe_async(operation.name(), async {
                let cancel = self.sites.cancellation();
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(TaskError::Closing.into()),
                    result = self.dispatch(operation) => result,
                }
            })
            .await
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
