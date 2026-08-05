use std::{collections::HashSet, io, path::Path, process::Command};

use serde::Serialize;
use tauri::{
    image::Image,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, WebviewUrl,
    WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;

use crate::{application::ApplicationSnapshot, domain::ProbeStatus, startup::BackendState};

const STATUS_WINDOW_LABEL: &str = "status-panel";
const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "relaypulse-status";
const MAIN_WINDOW_MENU_ID: &str = "main-window";
const QUIT_MENU_ID: &str = "quit";

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
        .icon(status_icon(TrayHealth::Neutral))
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
                let _ = show_main_window(app, "sites");
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
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_icon(Some(status_icon(tray_health(snapshot))))?;
    }
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        let view = if snapshot_has_alerts(snapshot) {
            "tray-alerts"
        } else {
            "tray-healthy"
        };
        window.emit("relaypulse://navigate", NavigateEvent { view })?;
    }
    Ok(())
}

fn show_status_panel(
    app: &AppHandle,
    has_alerts: bool,
    anchor: StatusPanelAnchor,
) -> tauri::Result<()> {
    let (view, width, height) = if has_alerts {
        ("tray-alerts", 440.0, 380.0)
    } else {
        ("tray-healthy", 400.0, 230.0)
    };
    let position = status_panel_position(app, anchor, width, height)?;
    if let Some(window) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        window.set_size(LogicalSize::new(width, height))?;
        window.set_position(position)?;
        window.emit("relaypulse://navigate", NavigateEvent { view })?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        STATUS_WINDOW_LABEL,
        WebviewUrl::App(format!("index.html?view={view}").into()),
    )
    .title("RelayPulse 状态")
    .inner_size(width, height)
    .min_inner_size(width, height)
    .max_inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()?;
    window.set_position(position)?;
    window.show()?;
    window.set_focus()?;
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

pub fn show_main_window(app: &AppHandle, view: &str) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.emit("relaypulse://navigate", NavigateEvent { view })?;
        window.show()?;
        window.set_focus()?;
    }
    if let Some(status_panel) = app.get_webview_window(STATUS_WINDOW_LABEL) {
        status_panel.hide()?;
    }
    Ok(())
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
    let latest = snapshot
        .latest_model_statuses
        .iter()
        .map(|status| (status.model_id.clone(), status.status))
        .collect::<std::collections::HashMap<_, _>>();
    snapshot.sites.iter().any(|site| {
        site.site.enabled
            && site.models.iter().any(|model| {
                model.model.enabled
                    && latest
                        .get(&model.model.id)
                        .is_some_and(|status| *status != ProbeStatus::Healthy)
            })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayHealth {
    Neutral,
    Healthy,
    Warning,
    Error,
}

fn tray_health(snapshot: &ApplicationSnapshot) -> TrayHealth {
    let enabled_models = snapshot
        .sites
        .iter()
        .filter(|site| site.site.enabled)
        .flat_map(|site| site.models.iter())
        .filter(|model| model.model.enabled)
        .map(|model| model.model.id.clone())
        .collect::<HashSet<_>>();
    if enabled_models.is_empty() {
        return TrayHealth::Neutral;
    }
    let statuses = snapshot
        .latest_model_statuses
        .iter()
        .filter(|status| enabled_models.contains(&status.model_id))
        .map(|status| status.status)
        .collect::<Vec<_>>();
    if statuses.iter().any(|status| {
        !matches!(
            status,
            ProbeStatus::Healthy | ProbeStatus::Slow | ProbeStatus::Unknown
        )
    }) {
        TrayHealth::Error
    } else if statuses.contains(&ProbeStatus::Slow) {
        TrayHealth::Warning
    } else if statuses.len() == enabled_models.len()
        && statuses
            .iter()
            .all(|status| *status == ProbeStatus::Healthy)
    {
        TrayHealth::Healthy
    } else {
        TrayHealth::Neutral
    }
}

fn status_icon(health: TrayHealth) -> Image<'static> {
    const SIZE: u32 = 18;
    let color = match health {
        TrayHealth::Neutral => [142, 142, 147, 255],
        TrayHealth::Healthy => [52, 199, 89, 255],
        TrayHealth::Warning => [255, 159, 10, 255],
        TrayHealth::Error => [255, 69, 58, 255],
    };
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    let center = (f64::from(SIZE) - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = f64::from(x) - center;
            let dy = f64::from(y) - center;
            if dx * dx + dy * dy <= 56.25 {
                let offset = ((y * SIZE + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

fn open_current_status_panel(app: AppHandle, anchor: StatusPanelAnchor) {
    if app
        .get_webview_window(STATUS_WINDOW_LABEL)
        .and_then(|window| window.is_visible().ok().map(|visible| (window, visible)))
        .is_some_and(|(window, visible)| visible && window.hide().is_ok())
    {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let service = app
            .try_state::<BackendState>()
            .and_then(|state| state.application_service().cloned());
        let has_alerts = match service {
            Some(service) => service
                .snapshot()
                .await
                .map(|snapshot| snapshot_has_alerts(&snapshot))
                .unwrap_or(false),
            None => false,
        };
        if show_status_panel(&app, has_alerts, anchor).is_ok() {
            if let Some(state) = app.try_state::<BackendState>() {
                if let Some(diagnostics) = state.diagnostics() {
                    diagnostics.record_window_opened("status-panel");
                }
            }
        }
    });
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
}
