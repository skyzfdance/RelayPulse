use super::actions as OperationAction;
use super::{GeneralSettings, SettingsStore, StorageError};
use crate::modules::operation_logs::*;
use crate::shared::diagnostics::{DiagnosticFailure, Diagnostics, Failure, Recorder};
use crate::shared::state_changes::StateChanges;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("invalid configuration input for {field}")]
    InvalidInput { field: &'static str },
    #[error("configuration storage failed: {0}")]
    Storage(#[source] StorageError),
}

impl DiagnosticFailure for ConfigurationError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::InvalidInput { .. } => Failure::Validation,
            Self::Storage(_) => Failure::Storage,
        }
    }
}

pub enum ConfigurationBoundaryFailure {
    InvalidRequest,
}

pub struct ConfigurationService {
    diagnostics: Recorder,
    operation_logs: OperationRecorder,
    mutation: std::sync::Mutex<()>,
    store: Box<dyn SettingsStore>,
    saved: watch::Sender<()>,
    changes: Arc<StateChanges>,
}

impl ConfigurationService {
    pub fn new(store: impl SettingsStore + 'static) -> Self {
        Self {
            store: Box::new(store),
            diagnostics: Recorder::default(),
            operation_logs: Default::default(),
            mutation: Default::default(),
            saved: watch::channel(()).0,
            changes: Arc::default(),
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Arc<Diagnostics>) -> Self {
        self.diagnostics = Recorder::new(diagnostics);
        self
    }

    pub fn with_operation_logs(mut self, logs: Arc<OperationLogs>) -> Self {
        self.operation_logs = OperationRecorder::new(logs);
        self
    }
    pub fn record_boundary_failure(&self, failure: ConfigurationBoundaryFailure) {
        let reason = match failure {
            ConfigurationBoundaryFailure::InvalidRequest => "设置字段缺失、类型错误或含未知字段。",
        };
        self.operation_logs.record_best_effort(OperationEvent::new(
            OperationAction::GeneralSettingsUpdated,
            OperationSource::User,
            OperationResult::Failure,
            "通用设置",
            "设置请求格式不合法",
            OperationLogContent::Failure {
                activity: "保存通用设置".into(),
                target: "通用设置".into(),
                reason: reason.into(),
            },
        ));
    }
    pub fn record_read_failure(&self) {
        self.operation_logs.record_best_effort(OperationEvent::new(
            OperationAction::PageLoadFailed,
            OperationSource::User,
            OperationResult::Failure,
            "通用设置",
            "无法读取通用设置",
            OperationLogContent::Failure {
                activity: "页面加载".into(),
                target: "通用设置".into(),
                reason: "本地设置不可用。".into(),
            },
        ));
    }
    pub fn with_changes(mut self, changes: Arc<StateChanges>) -> Self {
        self.changes = changes;
        self
    }

    pub fn load(&self) -> Result<GeneralSettings, ConfigurationError> {
        self.diagnostics.observe("configuration_load", || {
            let settings = self.store.load().map_err(ConfigurationError::Storage)?;
            // 非法的已有设置应阻止就绪，不以默认值覆盖。
            settings.validate()?;
            Ok(settings)
        })
    }

    /// 仅通知保存成功，不缓存设置；消费者收到通知后读取已提交的数据。
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.saved.subscribe()
    }

    pub fn save(&self, settings: GeneralSettings) -> Result<GeneralSettings, ConfigurationError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        let before = self.store.load().ok();
        let result = self.diagnostics.observe("configuration_save", || {
            settings.validate()?;
            // 全量保存六个字段，返回本次提交的完整结果，不依赖全局快照或 revision。
            // 此处不执行探针清理、日志清理或系统自启动等副作用。
            let _write = self.changes.begin();
            let saved = self
                .store
                .save(&settings)
                .map_err(ConfigurationError::Storage)?;
            self.saved.send_replace(());
            Ok(saved)
        });
        let event = match &result {
            Ok(saved) => OperationEvent::new(
                OperationAction::GeneralSettingsUpdated,
                OperationSource::User,
                OperationResult::Success,
                "通用设置",
                "已保存通用设置",
                OperationLogContent::Changes {
                    changes: changes(&before, saved),
                },
            ),
            Err(_) => OperationEvent::new(
                OperationAction::GeneralSettingsUpdated,
                OperationSource::User,
                OperationResult::Failure,
                "通用设置",
                "保存通用设置失败",
                OperationLogContent::Failure {
                    activity: "保存通用设置".into(),
                    target: "通用设置".into(),
                    reason: "设置不合法或本地存储不可用。".into(),
                },
            ),
        };
        self.operation_logs.record_best_effort(event);
        result
    }
}
