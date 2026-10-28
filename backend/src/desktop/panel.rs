//! 只把状态失效通知发送给面板宿主；不创建窗口，不广播业务事件。
use crate::application::snapshots::panel::Panel;
use serde::Serialize;
use std::sync::Mutex;
use tauri::async_runtime::JoinHandle;
use tauri::{Emitter, EventTarget, Manager, Runtime};

pub(crate) const STATE_CHANGED: &str = "panel:state-changed";
pub(crate) const WINDOW_LABEL: &str = "status-panel";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateChanged {
    revision: u64,
}

pub(super) struct PanelEvents {
    task: Mutex<Option<JoinHandle<()>>>,
}
impl PanelEvents {
    pub(super) fn start<R: Runtime>(app: tauri::AppHandle<R>, panel: &Panel) -> Self {
        let mut changes = panel.changes().subscribe();
        let task = tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                let revision = changes.borrow_and_update().revision;
                for (label, window) in app.webview_windows() {
                    // 托盘创建的独立窗口使用固定标签；开发时主窗口进入 /panel 也能接收。
                    let is_panel = label == WINDOW_LABEL
                        || window
                            .url()
                            .is_ok_and(|url| url.path().trim_end_matches('/') == "/panel");
                    if is_panel {
                        if let Err(error) = app.emit_to(
                            EventTarget::webview_window(label),
                            STATE_CHANGED,
                            StateChanged { revision },
                        ) {
                            eprintln!("[panel] state notification failed: {error}");
                        }
                    }
                }
            }
        });
        Self {
            task: Mutex::new(Some(task)),
        }
    }
    pub(super) async fn stop(&self) {
        let task = self
            .task
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for PanelEvents {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/integration/panel/events.rs"]
mod tests;
