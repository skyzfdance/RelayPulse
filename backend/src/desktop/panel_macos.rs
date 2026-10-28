//! macOS 非激活面板；保持原窗口代理，不抢占应用激活或唤起主窗口。
use tauri::{Manager, Runtime, WebviewWindow};
use tauri_nspanel::{
    objc2::MainThreadMarker as MacMainThread,
    objc2_app_kit::{NSApplication, NSEvent as MacEvent, NSEventType, NSWindowStyleMask},
    tauri_panel, ManagerExt, WebviewWindowExt,
};

tauri_panel! {
    panel!(RelayPulseStatusPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            is_floating_panel: true,
            hides_on_deactivate: false
        }
    })
}

pub(super) fn show<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let window = window.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let app = window.app_handle().clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            // 在主线程执行前可能已收到关闭动作，晚到的尺寸同步不能重新弹出。
            if !window
                .state::<super::panel_window::PanelWindow>()
                .should_show()
            {
                return Ok(());
            }
            let panel = match window.app_handle().get_webview_panel(window.label()) {
                Ok(panel) => panel,
                Err(_) => {
                    let panel = window.to_panel::<RelayPulseStatusPanel<R>>()?;
                    panel
                        .add_style_mask(NSWindowStyleMask::NonactivatingPanel)
                        .map_err(std::io::Error::other)?;
                    panel
                }
            };
            panel.show_and_make_key();
            Ok(())
        })();
        let _ = send.send(result);
    })?;
    receive
        .recv()
        .map_err(|_| tauri::Error::FailedToReceiveMessage)?
}

pub(super) fn double_click_interval() -> std::time::Duration {
    std::time::Duration::from_secs_f64(MacEvent::doubleClickInterval())
}

/// 原生焦点事件可能早于托盘回调；仅鼠标按下/抬起期间允许暂缓失焦隐藏。
pub(super) fn handling_mouse_click() -> bool {
    let Some(main_thread) = MacMainThread::new() else {
        return false;
    };
    NSApplication::sharedApplication(main_thread)
        .currentEvent()
        .is_some_and(|event| {
            matches!(
                event.r#type(),
                NSEventType::LeftMouseDown | NSEventType::LeftMouseUp
            )
        })
}
