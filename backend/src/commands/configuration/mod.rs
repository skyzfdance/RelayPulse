//! 通用设置命令模块：解析协议入参，调用配置服务并转换领域错误。
use tauri::State;

use super::error::CommandError;
use crate::{
    modules::configuration::{ConfigurationError, ConfigurationService, GeneralSettings},
    startup::StartupState,
};

fn invalid_settings(field: &'static str) -> CommandError {
    CommandError::invalid(field, "设置格式或取值不合法，请检查后重试。")
}

impl From<ConfigurationError> for CommandError {
    fn from(error: ConfigurationError) -> Self {
        match error {
            ConfigurationError::InvalidInput { field } => invalid_settings(field),
            ConfigurationError::Storage(error) => {
                eprintln!("[configuration] {error}");
                Self {
                    code: "database_unavailable",
                    category: "storage",
                    message: "无法读写本地设置，请检查数据库后重试。".into(),
                    field: None,
                    retryable: true,
                    operation_id: None,
                }
            }
        }
    }
}

fn configuration(state: &StartupState) -> Result<&ConfigurationService, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(&app.configuration),
        StartupState::Blocked(error) => Err(CommandError {
            code: "backend_unavailable",
            category: "startup",
            message: format!("设置服务启动失败：{error}"),
            field: None,
            retryable: false,
            operation_id: None,
        }),
    }
}

#[tauri::command]
pub(super) fn get_general_settings(
    state: State<'_, StartupState>,
) -> Result<GeneralSettings, CommandError> {
    configuration(&state)?.load().map_err(Into::into)
}

#[tauri::command]
pub(super) fn update_general_settings(
    state: State<'_, StartupState>,
    settings: serde_json::Value,
) -> Result<GeneralSettings, CommandError> {
    let service = configuration(&state)?;
    // 在协议边界解码，以结构化错误返回字段缺失、额外字段或类型错误。
    let settings = serde_json::from_value(settings).map_err(|_| invalid_settings("settings"))?;
    service.save(settings).map_err(Into::into)
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/configuration.rs"]
mod tests;
