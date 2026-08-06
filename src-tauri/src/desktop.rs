use std::{
    io,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::Duration,
};

use serde::Serialize;
use tauri::{
    image::Image,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::{Effect, EffectState, EffectsBuilder},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;

use crate::{application::ApplicationSnapshot, domain::ProbeStatus, startup::BackendState};

const STATUS_WINDOW_LABEL: &str = "status-panel";
const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "relaypulse-status";
const MAIN_WINDOW_MENU_ID: &str = "main-window";
const QUIT_MENU_ID: &str = "quit";
const STATUS_PANEL_OPENING_EVENT: &str = "relaypulse://status-panel-opening";
const STATUS_PANEL_CLOSING_EVENT: &str = "relaypulse://status-panel-closing";
const STATUS_PANEL_CLOSE_DELAY: Duration = Duration::from_millis(110);
static STATUS_PANEL_MOTION_GENERATION: AtomicU64 = AtomicU64::new(0);
static STATUS_PANEL_CLOSE_REQUESTED: AtomicBool = AtomicBool::new(false);
static STATUS_PANEL_OPEN_PENDING: AtomicBool = AtomicBool::new(false);
static STATUS_PANEL_ALERT_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct StatusPanelAnchor {
    click_position: PhysicalPosition<f64>,
    tray_rect: Rect,
}

pub fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let current = manager.is_enabled().map_err(|error| error.to_string())?;
    if current == enabled {
        return Ok(());
    }
    if enabled {
        manager.enable().map_err(|error| error.to_string())
    } else {
        manager.disable().map_err(|error| error.to_string())
    }
}

#[derive(Clone, Serialize)]
struct NavigateEvent<'a> {
    view: &'a str,
}

pub fn set_tray_enabled(app: &AppHandle, enabled: bool) -> tauri::Result<()> {
    if !enabled {
        STATUS_PANEL_CLOSE_REQUESTED.store(true, Ordering::Relaxed);
        STATUS_PANEL_MOTION_GENERATION.fetch_add(1, Ordering::Relaxed);
        if let Some(panel) = app.get_webview_window(STATUS_WINDOW_LABEL) {
            panel.hide()?;
        }
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        return tray.set_visible(enabled);
    }
    if !enabled {
        return Ok(());
    }

    let menu = MenuBuilder::new(app)
        .text(MAIN_WINDOW_MENU_ID, "打开主窗口")
        .separator()
        .text(QUIT_MENU_ID, "退出 RelayPulse")
        .build()?;
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .icon(status_icon(TrayIconState::Idle))
        .show_menu_on_left_click(false)
        .tooltip("RelayPulse")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                position,
                rect,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open_current_status_panel(
                    tray.app_handle().clone(),
                    StatusPanelAnchor {
                        click_position: position,
                        tray_rect: rect,
                    },
                );
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            MAIN_WINDOW_MENU_ID => {
                let _ = show_main_window(app);
            }
            QUIT_MENU_ID => app.exit(0),
            _ => {}
        });
    builder.build(app)?;
    Ok(())
}

pub fn update_status_projection(
    app: &AppHandle,
    snapshot: &ApplicationSnapshot,
) -> tauri::Result<()> {
    let alert_count = snapshot_alert_count(snapshot);
    STATUS_PANEL_ALERT_COUNT.store(alert_count, Ordering::Release);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_icon(Some(status_icon(tray_icon_state(snapshot))))?;
    }
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        let (view, width, height) = status_panel_layout(alert_count);
        window.set_size(LogicalSize::new(width, height))?;
        app.emit_to(
            STATUS_WINDOW_LABEL,
            "relaypulse://navigate",
            NavigateEvent { view },
        )?;
    }
    Ok(())
}

pub fn prewarm_status_panel(app: &AppHandle, snapshot: &ApplicationSnapshot) -> tauri::Result<()> {
    let alert_count = snapshot_alert_count(snapshot);
    STATUS_PANEL_ALERT_COUNT.store(alert_count, Ordering::Release);
    status_panel_window(app, alert_count).map(|_| ())
}

fn show_status_panel(
    app: &AppHandle,
    alert_count: usize,
    anchor: StatusPanelAnchor,
) -> tauri::Result<()> {
    let (view, width, height) = status_panel_layout(alert_count);
    let position = status_panel_position(app, anchor, width, height)?;
    let window = status_panel_window(app, alert_count)?;
    window.set_size(LogicalSize::new(width, height))?;
    window.set_position(position)?;
    app.emit_to(
        STATUS_WINDOW_LABEL,
        "relaypulse://navigate",
        NavigateEvent { view },
    )?;
    begin_status_panel_opening(&window)?;
    window.show()?;
    window.set_focus()?;
    Ok(())
}

fn status_panel_window(app: &AppHandle, alert_count: usize) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        return Ok(window);
    }

    let (view, width, height) = status_panel_layout(alert_count);
    let window = WebviewWindowBuilder::new(
        app,
        STATUS_WINDOW_LABEL,
        WebviewUrl::App(format!("index.html?view={view}").into()),
    )
    .title("RelayPulse 状态")
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .effects(
        EffectsBuilder::new()
            .effects([Effect::Popover, Effect::Acrylic])
            .state(EffectState::Active)
            .radius(10.0)
            .build(),
    )
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;
    Ok(window)
}

fn status_panel_layout(alert_count: usize) -> (&'static str, f64, f64) {
    if alert_count == 0 {
        ("tray-healthy", 368.0, 180.0)
    } else {
        let visible_alerts = alert_count.min(3) as f64;
        ("tray-alerts", 404.0, 182.0 + visible_alerts * 64.0)
    }
}

fn begin_status_panel_opening(window: &WebviewWindow) -> tauri::Result<()> {
    STATUS_PANEL_CLOSE_REQUESTED.store(false, Ordering::Relaxed);
    STATUS_PANEL_MOTION_GENERATION.fetch_add(1, Ordering::Relaxed);
    window
        .app_handle()
        .emit_to(STATUS_WINDOW_LABEL, STATUS_PANEL_OPENING_EVENT, ())
}

fn schedule_status_panel_hide(
    window: &WebviewWindow,
    hide_even_if_focused: bool,
) -> tauri::Result<()> {
    STATUS_PANEL_CLOSE_REQUESTED.store(true, Ordering::Relaxed);
    let generation = STATUS_PANEL_MOTION_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    window
        .app_handle()
        .emit_to(STATUS_WINDOW_LABEL, STATUS_PANEL_CLOSING_EVENT, ())?;
    let window = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STATUS_PANEL_CLOSE_DELAY).await;
        if STATUS_PANEL_MOTION_GENERATION.load(Ordering::Relaxed) != generation {
            return;
        }
        let should_hide = hide_even_if_focused || !window.is_focused().unwrap_or(false);
        if should_hide {
            let _ = window.hide();
        }
    });
    Ok(())
}

pub fn request_status_panel_hide(app: &AppHandle, hide_even_if_focused: bool) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        schedule_status_panel_hide(&window, hide_even_if_focused)?;
    }
    Ok(())
}

fn status_panel_position(
    app: &AppHandle,
    anchor: StatusPanelAnchor,
    width: f64,
    height: f64,
) -> tauri::Result<PhysicalPosition<i32>> {
    let monitor = app.monitor_from_point(anchor.click_position.x, anchor.click_position.y)?;
    let scale_factor = monitor
        .as_ref()
        .map_or(1.0, |monitor| monitor.scale_factor());
    let tray_position = anchor.tray_rect.position.to_physical::<f64>(scale_factor);
    let tray_size = anchor.tray_rect.size.to_physical::<f64>(scale_factor);
    let panel_width = width * scale_factor;
    let panel_height = height * scale_factor;
    let gap = 6.0 * scale_factor;

    let preferred_x = tray_position.x + (tray_size.width - panel_width) / 2.0;
    let preferred_y = tray_position.y + tray_size.height + gap;
    let (x, y) = match monitor {
        Some(monitor) => {
            let work_area = monitor.work_area();
            let margin = 8.0 * scale_factor;
            let min_x = f64::from(work_area.position.x) + margin;
            let min_y = f64::from(work_area.position.y) + margin;
            let max_x = (f64::from(work_area.position.x) + f64::from(work_area.size.width)
                - panel_width
                - margin)
                .max(min_x);
            let max_y = (f64::from(work_area.position.y) + f64::from(work_area.size.height)
                - panel_height
                - margin)
                .max(min_y);
            let y = if preferred_y <= max_y {
                preferred_y.max(min_y)
            } else {
                (tray_position.y - panel_height - gap).clamp(min_y, max_y)
            };
            (preferred_x.clamp(min_x, max_x), y)
        }
        None => (preferred_x, preferred_y),
    };

    Ok(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

pub fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.show()?;
        window.set_focus()?;
    }
    if let Some(status_panel) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        schedule_status_panel_hide(&status_panel, true)?;
    }
    Ok(())
}

pub fn show_main_window_at(app: &AppHandle, view: &str) -> tauri::Result<()> {
    app.emit_to(
        MAIN_WINDOW_LABEL,
        "relaypulse://navigate",
        NavigateEvent { view },
    )?;
    show_main_window(app)
}

pub fn open_directory(path: &Path) -> io::Result<()> {
    let mut command = Command::new(directory_opener());
    command.arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn directory_opener() -> &'static str {
    "open"
}

#[cfg(target_os = "windows")]
fn directory_opener() -> &'static str {
    "explorer"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn directory_opener() -> &'static str {
    "xdg-open"
}

pub fn snapshot_has_alerts(snapshot: &ApplicationSnapshot) -> bool {
    snapshot_alert_count(snapshot) > 0
}

fn snapshot_alert_count(snapshot: &ApplicationSnapshot) -> usize {
    let latest = snapshot
        .latest_model_statuses
        .iter()
        .map(|status| (status.model_id.clone(), status.status))
        .collect::<std::collections::HashMap<_, _>>();
    snapshot
        .sites
        .iter()
        .filter(|site| site.site.enabled)
        .flat_map(|site| site.models.iter())
        .filter(|model| model.model.enabled)
        .filter_map(|model| latest.get(&model.model.id))
        .filter(|status| {
            !matches!(
                status,
                ProbeStatus::Healthy | ProbeStatus::Unknown | ProbeStatus::Probing
            )
        })
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconState {
    Idle,
    Active,
}

fn tray_icon_state(snapshot: &ApplicationSnapshot) -> TrayIconState {
    let has_enabled_model = snapshot
        .sites
        .iter()
        .filter(|site| site.site.enabled)
        .flat_map(|site| site.models.iter())
        .any(|model| model.model.enabled);
    if has_enabled_model {
        TrayIconState::Active
    } else {
        TrayIconState::Idle
    }
}

fn status_icon(state: TrayIconState) -> Image<'static> {
    const SIZE: u32 = 64;
    let color = match state {
        TrayIconState::Idle => [28, 28, 30, 255],
        TrayIconState::Active => [22, 119, 255, 255],
    };
    let mut rgba = include_bytes!("../icons/tray-logo.rgba").to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel[3] != 0 {
            pixel[..3].copy_from_slice(&color[..3]);
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

fn open_current_status_panel(app: AppHandle, anchor: StatusPanelAnchor) {
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        if window.is_visible().unwrap_or(false) {
            if STATUS_PANEL_CLOSE_REQUESTED.load(Ordering::Relaxed) {
                let _ = begin_status_panel_opening(&window);
                let _ = window.show();
                let _ = window.set_focus();
                return;
            }
            let _ = schedule_status_panel_hide(&window, true);
            return;
        }
    }

    if STATUS_PANEL_OPEN_PENDING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        STATUS_PANEL_MOTION_GENERATION.fetch_add(1, Ordering::AcqRel);
        return;
    }
    let generation = STATUS_PANEL_MOTION_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;

    let alert_count = STATUS_PANEL_ALERT_COUNT.load(Ordering::Acquire);
    let opened = STATUS_PANEL_MOTION_GENERATION.load(Ordering::Acquire) == generation
        && show_status_panel(&app, alert_count, anchor).is_ok();
    STATUS_PANEL_OPEN_PENDING.store(false, Ordering::Release);
    if opened {
        if let Some(state) = app.try_state::<BackendState>() {
            if let Some(diagnostics) = state.diagnostics() {
                diagnostics.record_window_opened(None, "status-panel");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_native_directory_opener() {
        #[cfg(target_os = "macos")]
        assert_eq!(directory_opener(), "open");
        #[cfg(target_os = "windows")]
        assert_eq!(directory_opener(), "explorer");
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(directory_opener(), "xdg-open");
    }

    #[test]
    fn tray_logo_preserves_transparency_and_uses_the_monitoring_color() {
        for (state, expected_color) in [
            (TrayIconState::Idle, [28, 28, 30]),
            (TrayIconState::Active, [22, 119, 255]),
        ] {
            let icon = status_icon(state);
            assert_eq!((icon.width(), icon.height()), (64, 64));
            let pixels = icon.rgba().chunks_exact(4).collect::<Vec<_>>();
            assert!(pixels.iter().any(|pixel| pixel[3] == 0));
            assert!(pixels.iter().any(|pixel| (1..=254).contains(&pixel[3])));
            assert!(pixels.iter().any(|pixel| pixel[3] == 255));
            assert!(pixels
                .iter()
                .filter(|pixel| pixel[3] != 0)
                .all(|pixel| pixel[..3] == expected_color));
        }
    }

    #[test]
    fn status_panel_layout_matches_the_rendered_content() {
        assert_eq!(status_panel_layout(0), ("tray-healthy", 368.0, 180.0));
        assert_eq!(status_panel_layout(1), ("tray-alerts", 404.0, 246.0));
        assert_eq!(status_panel_layout(2), ("tray-alerts", 404.0, 310.0));
        assert_eq!(status_panel_layout(8), ("tray-alerts", 404.0, 374.0));
    }
}
