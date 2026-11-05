//! 小面板用例：组合公开查询，复用正式探测，不持有 SQL 或健康规则。
mod projection;
mod types;

use crate::{
    modules::{
        configuration::{ConfigurationError, ConfigurationService},
        monitoring::{Monitoring, MonitoringError},
        operation_logs::{
            OperationAction, OperationEvent, OperationLogContent, OperationLogs, OperationRecorder,
            OperationResult, OperationSource,
        },
        sites::{SiteError, SiteService},
    },
    shared::{
        diagnostics::{DiagnosticFailure, Diagnostics, Failure, Recorder},
        state_changes::StateChanges,
    },
};
use futures_util::{stream, StreamExt};
use std::sync::Arc;
pub(crate) use types::PanelSnapshot;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PanelError {
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
    #[error(transparent)]
    Site(#[from] SiteError),
    #[error(transparent)]
    Monitoring(#[from] MonitoringError),
    #[error("小面板正在刷新，请稍后重试。")]
    Busy,
    #[error("面板状态正在变化，请重试。")]
    Changing,
    #[error("无法读取探测时间。")]
    Timestamp,
}
impl DiagnosticFailure for PanelError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::Configuration(error) => error.diagnostic_failure(),
            Self::Site(error) => error.diagnostic_failure(),
            Self::Monitoring(error) => error.diagnostic_failure(),
            Self::Busy | Self::Changing => Failure::Conflict,
            Self::Timestamp => Failure::Storage,
        }
    }
}

pub(crate) struct Panel {
    configuration: Arc<ConfigurationService>,
    sites: Arc<SiteService>,
    monitoring: Arc<Monitoring>,
    changes: Arc<StateChanges>,
    refresh: tokio::sync::Mutex<()>,
    diagnostics: Recorder,
    operation_logs: OperationRecorder,
}
impl Panel {
    pub(crate) fn new(
        configuration: Arc<ConfigurationService>,
        sites: Arc<SiteService>,
        monitoring: Arc<Monitoring>,
        diagnostics: Arc<Diagnostics>,
        operation_logs: Arc<OperationLogs>,
    ) -> Self {
        Self {
            configuration,
            changes: sites.changes(),
            sites,
            monitoring,
            refresh: tokio::sync::Mutex::new(()),
            diagnostics: Recorder::new(diagnostics),
            operation_logs: OperationRecorder::new(operation_logs),
        }
    }
    pub(crate) fn changes(&self) -> &Arc<StateChanges> {
        &self.changes
    }
    pub(crate) async fn snapshot(&self) -> Result<PanelSnapshot, PanelError> {
        self.diagnostics
            .observe_async("panel_snapshot", self.read())
            .await
    }
    async fn read(&self) -> Result<PanelSnapshot, PanelError> {
        let cancellation = self.monitoring.cancellation();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(SiteError::Task(crate::modules::sites::TaskError::Closing).into()),
            result = self.read_consistent() => result,
        }
    }
    async fn read_consistent(&self) -> Result<PanelSnapshot, PanelError> {
        let mut changes = self.changes.subscribe();
        let mut retries = 0;
        loop {
            let before = *changes.borrow_and_update();
            if before.writers != 0 {
                changes.changed().await.map_err(|_| PanelError::Changing)?;
                continue;
            }
            let sites = self.sites.list().await?;
            let states = self
                .monitoring
                .states(
                    sites
                        .iter()
                        .flat_map(|site| site.models.iter().map(|model| model.id.clone()))
                        .collect(),
                )
                .await?;
            let settings = self.configuration.load()?;
            let activity = self.monitoring.activity();
            if before == self.changes.version() {
                return projection::project(
                    before.revision,
                    &sites,
                    &states,
                    &activity,
                    settings.continue_in_background,
                    settings.theme,
                );
            }
            retries += 1;
            if retries >= 16 {
                return Err(PanelError::Changing);
            }
        }
    }
    /// 等待所选站点批次全部结束；某站点失败不丢弃其他已开始的 future。
    pub(crate) async fn refresh(&self) -> Result<PanelSnapshot, PanelError> {
        self.diagnostics
            .observe_async("panel_refresh", async {
                let _refresh = self.refresh.try_lock().map_err(|_| {
                    self.record_refresh_preflight(
                        OperationResult::Blocked,
                        &PanelError::Busy.to_string(),
                    );
                    PanelError::Busy
                })?;
                let sites = self.sites.list().await.inspect_err(|error| {
                    let result = match error {
                        SiteError::Task(crate::modules::sites::TaskError::Busy) => {
                            OperationResult::Blocked
                        }
                        SiteError::Task(crate::modules::sites::TaskError::Closing) => {
                            OperationResult::Cancelled
                        }
                        _ => OperationResult::Failure,
                    };
                    self.record_refresh_preflight(result, &error.to_string());
                })?;
                let targets = sites
                    .into_iter()
                    .filter(|site| site.enabled && site.models.iter().any(|model| model.enabled))
                    .map(|site| site.id)
                    .collect::<Vec<_>>();
                if targets.is_empty() {
                    self.record_refresh_preflight(
                        OperationResult::Blocked,
                        "没有启用的站点或模型，未生成正式探测记录。",
                    );
                }
                let results =
                    stream::iter(targets.into_iter().map(|id| self.monitoring.run(id, None)))
                        .buffer_unordered(8)
                        .collect::<Vec<_>>()
                        .await;
                for result in results {
                    result?;
                }
                self.read().await.inspect_err(|error| {
                    self.record_refresh_read_failure(&error.to_string());
                })
            })
            .await
    }

    // 面板选择范围和组合查询的失败归本用例；正式执行的结果仍由 monitoring 记录。
    fn record_refresh_preflight(&self, result: OperationResult, reason: &str) {
        self.operation_logs.record_best_effort(OperationEvent::new(
            OperationAction::ManualProbe,
            OperationSource::User,
            result,
            "面板或托盘探测",
            "手动探测未能开始。",
            OperationLogContent::ProbeFailure {
                reason: reason.into(),
            },
        ));
    }

    fn record_refresh_read_failure(&self, reason: &str) {
        self.operation_logs.record_best_effort(OperationEvent::new(
            OperationAction::PageLoadFailed,
            OperationSource::User,
            OperationResult::Failure,
            "监测面板",
            "无法读取探测后的面板状态。",
            OperationLogContent::Failure {
                activity: "读取面板".into(),
                target: "监测面板".into(),
                reason: reason.into(),
            },
        ));
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/panel/projection.rs"]
mod projection_tests;
#[cfg(test)]
#[path = "../../../../tests/integration/panel/mod.rs"]
mod tests;
