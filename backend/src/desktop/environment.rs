//! 宿主适配：从当前 Tauri 身份与系统路径 API 取值，不自行拼平台主目录。

use tauri::{AppHandle, Manager};

use crate::shared::environment::{EnvironmentContext, EnvironmentError};

pub(super) fn resolve(app: &AppHandle) -> Result<EnvironmentContext, EnvironmentError> {
    let identifier = &app.config().identifier;
    // 身份不合法时，不继续解析或检查数据目录。
    crate::application::environment::identify(identifier)?;
    let system_data_root = app
        .path()
        .data_dir()
        .map_err(|_| EnvironmentError::SystemDataDirectoryUnavailable("system data"))?;
    let app_data_root = app
        .path()
        .app_data_dir()
        .map_err(|_| EnvironmentError::SystemDataDirectoryUnavailable("application data"))?;
    crate::application::environment::resolve(identifier, &system_data_root, &app_data_root)
}
