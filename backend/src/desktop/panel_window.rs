//! 托盘点击的原生窗口叶子操作；复用 /panel，不执行探测或决定页面动作。
use super::panel::WINDOW_LABEL;
use crate::modules::configuration::ThemeMode;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{
    async_runtime::JoinHandle, AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalRect,
    Rect, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

#[derive(Default)]
pub(super) struct PanelWindow {
    pending: Mutex<Option<JoinHandle<()>>>,
    closing: AtomicBool,
    presentation: Mutex<Presentation>,
}

#[derive(Default)]
struct Presentation {
    content_height: Option<f64>,
    theme: Option<ThemeMode>,
    anchor: Option<Rect>,
    show_requested: bool,
    tray_pressed: bool,
    last_tray_click: Option<Instant>,
}

impl Presentation {
    fn tray_click(&mut self, now: Instant, double_click_interval: Duration) -> Option<bool> {
        let repeated = self.last_tray_click.is_some_and(|previous| {
            now.saturating_duration_since(previous) <= double_click_interval
        });
        self.last_tray_click = Some(now);
        if repeated {
            // 双击的后半次只恢复面板焦点，不再次翻转开关。
            return self.show_requested.then_some(false);
        }
        Some(true)
    }

    fn dismiss_on_blur(&mut self, pointer_on_tray_click: bool) -> bool {
        if !self.show_requested || self.tray_pressed || pointer_on_tray_click {
            return false;
        }
        self.show_requested = false;
        self.last_tray_click = None;
        true
    }
}

impl PanelWindow {
    pub(super) fn should_show(&self) -> bool {
        !self.closing.load(Ordering::SeqCst)
            && self
                .presentation
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .show_requested
    }

    pub(super) fn tray_pressed(&self, pressed: bool) {
        self.presentation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tray_pressed = pressed;
    }

    pub(super) fn request<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        anchor: Option<Rect>,
        toggle: bool,
    ) {
        let toggle = if toggle {
            #[cfg(target_os = "macos")]
            let interval = super::panel_macos::double_click_interval();
            #[cfg(not(target_os = "macos"))]
            let interval = Duration::from_millis(500);
            let Some(toggle) = self
                .presentation
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .tray_click(Instant::now(), interval)
            else {
                return;
            };
            toggle
        } else {
            false
        };
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.closing.load(Ordering::SeqCst)
            || pending
                .as_ref()
                .is_some_and(|task| !task.inner().is_finished())
            || app.tray_by_id(super::tray::TRAY_ID).is_none()
        {
            return;
        }
        let anchor = match normalize_anchor(app, anchor) {
            Ok(anchor) => anchor,
            Err(_) => {
                eprintln!("[panel] tray anchor lookup failed");
                return;
            }
        };
        let app = app.clone();
        // WebView2 要求从异步任务创建窗口；持有句柄，退出时排空。
        *pending = Some(tauri::async_runtime::spawn(async move {
            if open(&app, anchor, toggle).is_err() {
                eprintln!("[panel] window operation failed");
            }
            if (app.state::<PanelWindow>().closing.load(Ordering::SeqCst)
                || app.tray_by_id(super::tray::TRAY_ID).is_none())
                && hide(&app).is_err()
            {
                eprintln!("[panel] window hide failed");
            }
        }));
    }

    pub(super) async fn stop(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let task = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl Drop for PanelWindow {
    fn drop(&mut self) {
        if let Some(task) = self
            .pending
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

pub(super) fn hide<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let state = app.state::<PanelWindow>();
    let mut presentation = state
        .presentation
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    presentation.show_requested = false;
    presentation.tray_pressed = false;
    presentation.last_tray_click = None;
    drop(presentation);
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide()?;
    }
    Ok(())
}

fn open<R: Runtime>(app: &AppHandle<R>, anchor: Option<Rect>, toggle: bool) -> tauri::Result<()> {
    open_with(app, anchor, toggle, |window, height| {
        fit(window, anchor, height)?;
        show(window)
    })
}

fn show<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        super::panel_macos::show(window)
    }
    #[cfg(not(target_os = "macos"))]
    {
        if !window.state::<PanelWindow>().should_show() {
            return Ok(());
        }
        window.show()?;
        window.set_focus()
    }
}

fn dismiss_on_blur<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    if window.is_focused()? {
        return Ok(());
    }
    let state = window.state::<PanelWindow>();
    let pointer_on_tray_click = pointer_on_tray_click(window)?;
    let dismissed = state
        .presentation
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .dismiss_on_blur(pointer_on_tray_click);
    if dismissed {
        window.hide()?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn pointer_on_tray_click<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<bool> {
    if !super::panel_macos::handling_mouse_click() {
        return Ok(false);
    }
    let app = window.app_handle();
    let primary = app.primary_monitor()?;
    let cursor = app.cursor_position()?.to_logical::<f64>(
        primary
            .as_ref()
            .map_or(1.0, |monitor| monitor.scale_factor()),
    );
    let anchor = app
        .state::<PanelWindow>()
        .presentation
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .anchor;
    Ok(anchor.is_some_and(|rect| {
        let position = rect.position.to_logical::<f64>(1.0);
        let size = rect.size.to_logical::<f64>(1.0);
        cursor.x >= position.x
            && cursor.x <= position.x + size.width
            && cursor.y >= position.y
            && cursor.y <= position.y + size.height
    }))
}

#[cfg(not(target_os = "macos"))]
fn pointer_on_tray_click<R: Runtime>(_window: &WebviewWindow<R>) -> tauri::Result<bool> {
    Ok(false)
}

#[cfg(target_os = "macos")]
fn normalize_anchor<R: Runtime>(
    app: &AppHandle<R>,
    anchor: Option<Rect>,
) -> tauri::Result<Option<Rect>> {
    let Some(anchor) = anchor else {
        return Ok(None);
    };
    let primary = app.primary_monitor()?;
    // Tao 的 macOS 屏幕查询使用全局逻辑点；全局鼠标物理坐标采用主屏缩放。
    // 托盘矩形却采用托盘所在屏幕缩放，不能用新建面板当前屏幕的倍率换算。
    let cursor = app.cursor_position()?.to_logical::<f64>(
        primary
            .as_ref()
            .map_or(1.0, |monitor| monitor.scale_factor()),
    );
    let monitor = app.monitor_from_point(cursor.x, cursor.y)?.or(primary);
    let scale = monitor
        .as_ref()
        .map_or(1.0, |monitor| monitor.scale_factor());
    Ok(Some(logical_tray_anchor(anchor, scale)))
}

#[cfg(target_os = "macos")]
fn logical_tray_anchor(anchor: Rect, scale: f64) -> Rect {
    Rect {
        position: anchor.position.to_logical::<f64>(scale).into(),
        size: anchor.size.to_logical::<f64>(scale).into(),
    }
}

#[cfg(not(target_os = "macos"))]
fn normalize_anchor<R: Runtime>(
    _app: &AppHandle<R>,
    anchor: Option<Rect>,
) -> tauri::Result<Option<Rect>> {
    Ok(anchor)
}

fn window_position(position: PhysicalPosition<i32>, target_scale: f64) -> tauri::Position {
    #[cfg(target_os = "macos")]
    {
        position.to_logical::<f64>(target_scale).into()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target_scale;
        position.into()
    }
}

fn fit<R: Runtime>(
    window: &WebviewWindow<R>,
    anchor: Option<Rect>,
    height: f64,
) -> tauri::Result<()> {
    let app = window.app_handle();
    let monitor = if let Some(rect) = anchor {
        #[cfg(target_os = "macos")]
        let position = rect.position.to_logical::<f64>(1.0);
        #[cfg(not(target_os = "macos"))]
        let position = rect.position.to_physical::<f64>(window.scale_factor()?);
        app.monitor_from_point(position.x, position.y)?
            .or(app.primary_monitor()?)
    } else {
        app.primary_monitor()?
    };
    if let Some(monitor) = monitor {
        let (size, position) =
            placement(monitor.work_area(), monitor.scale_factor(), anchor, height);
        window.set_size(size)?;
        // macOS 使用全局逻辑点定位，避免窗口还在另一倍率屏幕时再次换算物理坐标。
        window.set_position(window_position(position, monitor.scale_factor()))?;
    } else {
        window.set_size(LogicalSize::new(360.0, height))?;
        window.center()?;
    }
    Ok(())
}

/// 首次渲染（含失败重试界面）完成后才显示；隐藏期间只更新尺寸，不重新弹窗。
pub(crate) fn synchronize<R: Runtime>(
    window: &WebviewWindow<R>,
    height: f64,
    theme: ThemeMode,
) -> tauri::Result<bool> {
    let state = window.state::<PanelWindow>();
    let mut presentation = state
        .presentation
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let update_theme = presentation.theme != Some(theme);
    presentation.content_height = Some(height);
    let anchor = presentation.anchor;
    // 原生操作可能同步发出焦点回调；不能跨平台调用持有展示状态锁。
    drop(presentation);
    if update_theme {
        // 原生材质也遵循应用设置；System 清除覆盖，继续跟随系统外观。
        window.set_theme(match theme {
            ThemeMode::System => None,
            ThemeMode::Light => Some(tauri::Theme::Light),
            ThemeMode::Dark => Some(tauri::Theme::Dark),
        })?;
        state
            .presentation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .theme = Some(theme);
    }
    fit(window, anchor, height)?;
    if state.should_show() && !window.is_visible()? {
        show(window)?;
    }
    Ok(cfg!(target_os = "macos"))
}

fn open_with<R: Runtime>(
    app: &AppHandle<R>,
    anchor: Option<Rect>,
    toggle: bool,
    position: impl FnOnce(&WebviewWindow<R>, f64) -> tauri::Result<()>,
) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window
    } else {
        let builder = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("panel".into()))
            .title("RelayPulse 小面板")
            .inner_size(360.0, 360.0)
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .focused(false)
            .accept_first_mouse(true);
        #[cfg(target_os = "macos")]
        let builder = builder.effects(
            EffectsBuilder::new()
                .effect(Effect::Popover)
                .state(EffectState::Active)
                .radius(12.0)
                .build(),
        );
        let window = builder.build()?;
        let closing_window = window.clone();
        window.on_window_event(move |event| {
            let result = match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    hide(closing_window.app_handle())
                }
                WindowEvent::Focused(false) => dismiss_on_blur(&closing_window),
                _ => Ok(()),
            };
            if result.is_err() {
                eprintln!("[panel] window hide failed");
            }
        });
        window
    };
    let state = app.state::<PanelWindow>();
    let mut presentation = state
        .presentation
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if toggle && presentation.show_requested {
        presentation.show_requested = false;
        drop(presentation);
        return window.hide();
    }
    presentation.anchor = anchor;
    presentation.show_requested = true;
    let content_height = presentation.content_height;
    drop(presentation);
    // 新窗口先保持隐藏，首个有效内容高度到达后再定位和显示。
    if let Some(height) = content_height {
        position(&window, height)?;
    }
    Ok(())
}

/// 物理坐标定位，逻辑宽度 360px、高度随内容变化，支持负坐标与不同缩放屏幕。
fn placement(
    area: &PhysicalRect<i32, u32>,
    scale: f64,
    anchor: Option<Rect>,
    content_height: f64,
) -> (LogicalSize<f64>, PhysicalPosition<i32>) {
    let width = 360.0_f64.min(f64::from(area.size.width) / scale);
    let height = content_height.min(f64::from(area.size.height) / scale);
    let physical_width = (width * scale).round() as i64;
    let physical_height = (height * scale).round() as i64;
    let left = i64::from(area.position.x);
    let top = i64::from(area.position.y);
    let right = left + i64::from(area.size.width);
    let bottom = top + i64::from(area.size.height);
    let (x, y) = match anchor {
        Some(rect) => {
            let position = rect.position.to_physical::<i32>(scale);
            let size = rect.size.to_physical::<u32>(scale);
            let gap = (4.0 * scale).round() as i64;
            let below = i64::from(position.y) + i64::from(size.height) + gap;
            let y = if below + physical_height <= bottom {
                below
            } else {
                i64::from(position.y) - physical_height - gap
            };
            (
                i64::from(position.x) + i64::from(size.width) / 2 - physical_width / 2,
                y,
            )
        }
        None => (
            left + (right - left - physical_width) / 2,
            top + (bottom - top - physical_height) / 2,
        ),
    };
    (
        LogicalSize::new(width, height),
        PhysicalPosition::new(
            x.clamp(left, right - physical_width) as i32,
            y.clamp(top, bottom - physical_height) as i32,
        ),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/panel/interactions.rs"]
mod interaction_tests;
#[cfg(test)]
#[path = "../../tests/unit/panel/placement.rs"]
mod placement_tests;
#[cfg(test)]
#[path = "../../tests/integration/panel/window.rs"]
mod tests;
