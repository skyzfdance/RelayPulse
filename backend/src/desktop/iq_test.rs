//! 宿主只传递 IQ 失效版本，前端按版本重取快照。
use std::sync::Mutex;
use tauri::{Emitter, EventTarget, Runtime};
pub(crate) const STATE_CHANGED: &str = "iq-test:state-changed";
pub(super) struct IqEvents {
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
impl IqEvents {
    pub(super) fn start<R: Runtime>(
        app: tauri::AppHandle<R>,
        service: &crate::modules::iq_test::IqTest,
    ) -> Self {
        let mut changes = service.changes();
        let task = tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                let revision = *changes.borrow_and_update();
                if app
                    .emit_to(
                        EventTarget::webview_window("main"),
                        STATE_CHANGED,
                        serde_json::json!({"revision":revision}),
                    )
                    .is_err()
                {
                    eprintln!("[iq-test] state notification failed");
                }
            }
        });
        Self {
            task: Mutex::new(Some(task)),
        }
    }
    pub(super) async fn stop(&self) {
        let task = self.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for IqEvents {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/integration/iq_test/events.rs"]
mod tests;
