use super::{GeneralSettings, SettingsStore, StorageError};
use crate::shared::diagnostics::{DiagnosticFailure, Diagnostics, Failure, Recorder};
use crate::shared::state_changes::StateChanges;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigurationError {
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

pub(crate) struct ConfigurationService {
    diagnostics: Recorder,
    store: Box<dyn SettingsStore>,
    saved: watch::Sender<()>,
    changes: Arc<StateChanges>,
}

impl ConfigurationService {
    pub(crate) fn new(store: impl SettingsStore + 'static) -> Self {
        Self {
            store: Box::new(store),
            diagnostics: Recorder::default(),
            saved: watch::channel(()).0,
            changes: Arc::default(),
        }
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Arc<Diagnostics>) -> Self {
        self.diagnostics = Recorder::new(diagnostics);
        self
    }

    pub(crate) fn with_changes(mut self, changes: Arc<StateChanges>) -> Self {
        self.changes = changes;
        self
    }

    pub(crate) fn load(&self) -> Result<GeneralSettings, ConfigurationError> {
        self.diagnostics.observe("configuration_load", || {
            let settings = self.store.load().map_err(ConfigurationError::Storage)?;
            // 非法的已有设置应阻止就绪，不以默认值覆盖。
            settings.validate()?;
            Ok(settings)
        })
    }

    /// 仅通知保存成功，不缓存设置；消费者收到通知后读取已提交的数据。
    pub(crate) fn subscribe(&self) -> watch::Receiver<()> {
        self.saved.subscribe()
    }

    pub(crate) fn save(
        &self,
        settings: GeneralSettings,
    ) -> Result<GeneralSettings, ConfigurationError> {
        self.diagnostics.observe("configuration_save", || {
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
        })
    }
}
