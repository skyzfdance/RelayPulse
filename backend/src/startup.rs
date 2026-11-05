//! 启动编排：环境校验 → 资源组装 → 通用设置、通知与站点配置校验 → 宿主交接。
use crate::{
    application::composition::{self, Application},
    modules::{
        configuration::ConfigurationError,
        notifications::NotificationError,
        operation_logs::{defer_startup_failure, StartupFailureStage},
        sites::SiteError,
    },
    shared::{
        diagnostics::{Diagnostics, Failure, Outcome},
        environment::{EnvironmentContext, EnvironmentError},
        persistence::PersistenceError,
    },
};

use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum StartupError {
    #[error("environment initialization blocked: {0}")]
    Environment(#[from] EnvironmentError),
    #[error("persistence initialization blocked: {0}")]
    Persistence(#[from] PersistenceError),
    #[error("configuration initialization blocked: {0}")]
    Configuration(#[from] ConfigurationError),
    #[error("notification initialization blocked: {0}")]
    Notifications(#[from] NotificationError),
    #[error("site initialization blocked: {0}")]
    Sites(#[from] SiteError),
}

pub(crate) enum StartupState {
    /// 表示通用设置、通知与站点服务就绪；周期任务由宿主随后启动。
    Ready(Application),
    Blocked(StartupError),
}

/// 诊断资源独立于业务就绪状态，数据库初始化失败时仍交给宿主持有。
pub(crate) struct DiagnosticsState(pub Option<Arc<Diagnostics>>);

pub(crate) fn initialize(
    environment: Result<EnvironmentContext, EnvironmentError>,
) -> (StartupState, DiagnosticsState) {
    // 环境校验通过后才创建诊断资源，再进入数据库和业务校验阶段。
    let environment = match environment {
        Ok(environment) => environment,
        Err(error) => return (StartupState::Blocked(error.into()), DiagnosticsState(None)),
    };
    let startup_root = environment.paths().data_root().to_owned();
    let diagnostics = composition::diagnostics(&environment);
    diagnostics.record_best_effort("startup", Outcome::Started);
    let state = match prepare(environment, diagnostics.clone()) {
        Ok(application) => {
            diagnostics.record_best_effort("startup", Outcome::Succeeded);
            application
                .operation_logs
                .record_startup_ready(application.environment.paths());
            StartupState::Ready(application)
        }
        Err((error, stage)) => {
            diagnostics.record_best_effort("startup", Outcome::Failed(stage));
            let pending_stage = match &error {
                StartupError::Persistence(_) => StartupFailureStage::Persistence,
                StartupError::Configuration(_) => StartupFailureStage::Configuration,
                StartupError::Notifications(_) => StartupFailureStage::Notifications,
                StartupError::Sites(_) => StartupFailureStage::Sites,
                StartupError::Environment(_) => {
                    return (
                        StartupState::Blocked(error),
                        DiagnosticsState(Some(diagnostics)),
                    )
                }
            };
            defer_startup_failure(&startup_root, pending_stage);
            StartupState::Blocked(error)
        }
    };
    (state, DiagnosticsState(Some(diagnostics)))
}

fn prepare(
    environment: EnvironmentContext,
    diagnostics: Arc<Diagnostics>,
) -> Result<Application, (StartupError, Failure)> {
    // 只安排阶段顺序并标记失败阶段；资源创建和业务规则由各模块执行。
    let application = composition::compose(environment, diagnostics)
        .map_err(|error| (error.into(), Failure::Persistence))?;
    // 连接成功后仍需校验已有配置；缺失或非法数据不得用默认值覆盖。
    application
        .configuration
        .load()
        .map_err(|error| (error.into(), Failure::Configuration))?;
    application
        .notifications
        .check_ready()
        .map_err(|error| (error.into(), Failure::Notifications))?;
    application
        .sites
        .check_ready()
        .map_err(|error| (error.into(), Failure::Configuration))?;
    Ok(application)
}

#[cfg(test)]
#[path = "../tests/integration/startup/mod.rs"]
mod tests;
