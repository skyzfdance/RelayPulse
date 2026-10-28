//! 主窗口仅由明确的用户动作显示，不参与托盘面板的显示和聚焦。
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub(super) const NAVIGATE: &str = "main-window:navigate";

pub(super) fn show<R: Runtime>(app: &AppHandle<R>, route: Option<&str>) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("main") {
        if let Some(route) = route {
            // 已有 WebView 交给 Vue Router 导航，同页只刷新数据，不重载整个应用。
            app.emit_to(EventTarget::webview_window("main"), NAVIGATE, route)?;
        }
        window
    } else {
        let mut config = app
            .config()
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .cloned()
            .ok_or_else(|| std::io::Error::other("main window configuration missing"))?;
        if let Some(route) = route {
            config.url = WebviewUrl::App(route.trim_start_matches('/').into());
        }
        WebviewWindowBuilder::from_config(app, &config)?.build()?
    };
    window.unminimize()?;
    window.show()?;
    window.set_focus()?;
    super::panel_window::hide(app)
}

pub(crate) fn settings<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    show(app, Some("/config/general"))
}

pub(crate) fn sites<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    show(app, Some("/config/sites"))
}

pub(super) fn records<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    show(app, Some("/run/records"))
}
