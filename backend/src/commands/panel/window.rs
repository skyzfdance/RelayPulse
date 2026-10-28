//! 面板展示就绪、内容高度与主题同步，只调用原生窗口叶子能力。
use crate::{
    commands::error::CommandError, desktop::panel::WINDOW_LABEL, modules::configuration::ThemeMode,
};
use tauri::{Manager, Runtime, WebviewWindow};

#[tauri::command]
pub(in crate::commands) async fn open_panel_sites<R: Runtime>(
    window: WebviewWindow<R>,
) -> Result<(), CommandError> {
    if window.label() != WINDOW_LABEL {
        return Err(CommandError::invalid(
            "window",
            "仅独立小面板可以打开站点管理。",
        ));
    }
    crate::desktop::main_window::sites(window.app_handle()).map_err(|_| CommandError {
        code: "main_window_unavailable",
        category: "desktop",
        message: "无法打开站点管理，请重试。".into(),
        field: None,
        retryable: true,
        operation_id: None,
    })
}

#[tauri::command]
pub(in crate::commands) async fn open_panel_settings<R: Runtime>(
    window: WebviewWindow<R>,
) -> Result<(), CommandError> {
    if window.label() != WINDOW_LABEL {
        return Err(CommandError::invalid(
            "window",
            "仅独立小面板可以打开主窗口设置。",
        ));
    }
    crate::desktop::main_window::settings(window.app_handle()).map_err(|_| CommandError {
        code: "main_window_unavailable",
        category: "desktop",
        message: "无法打开主窗口设置，请重试。".into(),
        field: None,
        retryable: true,
        operation_id: None,
    })
}

#[tauri::command]
pub(in crate::commands) async fn sync_panel_window<R: Runtime>(
    window: WebviewWindow<R>,
    height: f64,
    theme: ThemeMode,
) -> Result<bool, CommandError> {
    if window.label() != WINDOW_LABEL {
        return Err(CommandError::invalid(
            "window",
            "仅独立小面板可以同步窗口尺寸。",
        ));
    }
    if !height.is_finite() || height <= 0.0 {
        return Err(CommandError::invalid("height", "面板高度必须是正数。"));
    }
    crate::desktop::panel_window::synchronize(&window, height, theme).map_err(|_| CommandError {
        code: "panel_window_unavailable",
        category: "desktop",
        message: "无法同步小面板窗口，请重新打开。".into(),
        field: None,
        retryable: true,
        operation_id: None,
    })
}
