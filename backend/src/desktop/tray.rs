//! 系统托盘适配：消费配置投影，在主线程更新图标，复用唯一面板窗口。
use super::panel_window::{self, PanelWindow};
use crate::application::snapshots::tray::{Tray, TrayState};
use std::sync::{Arc, Mutex};
use tauri::{
    async_runtime::JoinHandle,
    image::Image,
    menu::{MenuBuilder, MenuEvent},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

pub(super) const TRAY_ID: &str = "relaypulse-status";
const OPEN_PANEL: &str = "relaypulse-tray-panel";
const QUIT: &str = "relaypulse-tray-quit";

pub(super) struct TrayRuntime {
    task: Mutex<Option<JoinHandle<()>>>,
}

impl TrayRuntime {
    pub(super) fn start<R: Runtime>(app: AppHandle<R>, projection: Arc<Tray>) -> Self {
        Self {
            task: Mutex::new(Some(tauri::async_runtime::spawn(watch(
                projection,
                NativeTray(app),
            )))),
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

impl Drop for TrayRuntime {
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

trait Platform: Send + 'static {
    fn apply(&mut self, state: TrayState) -> tauri::Result<()>;
}

async fn watch(projection: Arc<Tray>, mut platform: impl Platform) {
    let mut changes = projection.changes().subscribe();
    let mut applied = None;
    loop {
        changes.borrow_and_update();
        match projection.snapshot().await {
            Ok(state) if applied != Some(state) => match platform.apply(state) {
                Ok(()) => applied = Some(state),
                Err(_) => {
                    applied = None;
                    eprintln!("[tray] system icon update failed");
                }
            },
            Ok(_) => {}
            Err(_) => eprintln!("[tray] configuration projection failed"),
        }
        if changes.changed().await.is_err() {
            break;
        }
    }
}

struct NativeTray<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Platform for NativeTray<R> {
    fn apply(&mut self, state: TrayState) -> tauri::Result<()> {
        let app = self.0.clone();
        let (send, receive) = std::sync::mpsc::channel();
        self.0.run_on_main_thread(move || {
            let _ = send.send(apply(&app, state));
        })?;
        receive
            .recv()
            .map_err(|_| tauri::Error::FailedToReceiveMessage)?
    }
}

impl<R: Runtime> Drop for NativeTray<R> {
    fn drop(&mut self) {
        let app = self.0.clone();
        // 图标最终释放也在主线程执行；正常 stop 等待转发任务退出后再退出宿主。
        let _ = self.0.run_on_main_thread(move || {
            drop(app.remove_tray_by_id(TRAY_ID));
        });
    }
}

fn icon(state: TrayState) -> Image<'static> {
    let color = if state == TrayState::Active {
        [22, 119, 255]
    } else {
        [115, 115, 115]
    };
    let mut rgba = include_bytes!("../../icons/tray-logo.rgba").to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[..3].copy_from_slice(&color);
    }
    Image::new_owned(rgba, 64, 64)
}

fn apply<R: Runtime>(app: &AppHandle<R>, state: TrayState) -> tauri::Result<()> {
    if state == TrayState::Hidden {
        drop(app.remove_tray_by_id(TRAY_ID));
        return panel_window::hide(app);
    }
    let template = state == TrayState::Default;
    let tooltip = if state == TrayState::Active {
        "RelayPulse · 配置有效"
    } else {
        "RelayPulse · 等待启用模型"
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        // macOS 同时更新图像与 template，避免两次渲染造成闪烁。
        tray.set_icon_with_as_template(Some(icon(state)), template)?;
        tray.set_tooltip(Some(tooltip))?;
    } else {
        let menu = MenuBuilder::new(app)
            .text(OPEN_PANEL, "打开状态面板")
            .separator()
            .text(QUIT, "退出 RelayPulse")
            .build()?;
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon(state))
            .icon_as_template(template)
            .tooltip(tooltip)
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    rect,
                    button: MouseButton::Left,
                    button_state,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    let panel = app.state::<PanelWindow>();
                    panel.tray_pressed(button_state == MouseButtonState::Down);
                    if button_state == MouseButtonState::Up {
                        panel.request(app, Some(rect), true);
                    }
                }
            })
            .build(app)?;
    }
    Ok(())
}

/// 只注册一次，避免反复关闭/开启图标时累积全局菜单监听。
pub(super) fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        OPEN_PANEL => {
            let anchor = app
                .tray_by_id(TRAY_ID)
                .and_then(|tray| tray.rect().ok().flatten());
            app.state::<PanelWindow>().request(app, anchor, false);
        }
        QUIT => app.exit(0),
        _ => {}
    }
}

#[cfg(test)]
#[path = "../../tests/unit/panel/tray_icon.rs"]
mod icon_tests;
#[cfg(test)]
#[path = "../../tests/integration/panel/tray_updates.rs"]
mod tests;
