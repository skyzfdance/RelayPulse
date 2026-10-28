use crate::{
    application::snapshots::panel::PanelError, commands::error::CommandError,
    modules::monitoring::MonitoringError, modules::sites::SiteError,
};

impl From<PanelError> for CommandError {
    fn from(error: PanelError) -> Self {
        let (code, category, message) = match error {
            PanelError::Configuration(error) => return error.into(),
            PanelError::Site(error) | PanelError::Monitoring(MonitoringError::Site(error)) => {
                return error.into()
            }
            PanelError::Monitoring(MonitoringError::Storage) | PanelError::Timestamp => {
                return SiteError::Storage.into()
            }
            PanelError::Monitoring(MonitoringError::Capture) => return SiteError::Capture.into(),
            PanelError::Monitoring(MonitoringError::Worker) => return SiteError::Worker.into(),
            PanelError::Busy => ("panel_busy", "conflict", "小面板正在刷新，请稍后重试。"),
            PanelError::Changing => (
                "panel_state_changed",
                "conflict",
                "面板状态正在变化，请重试。",
            ),
        };
        Self {
            code,
            category,
            message: message.into(),
            field: None,
            retryable: true,
            operation_id: None,
        }
    }
}
