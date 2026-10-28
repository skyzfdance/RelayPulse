use super::*;
use tauri::PhysicalSize;

fn area(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect<i32, u32> {
    PhysicalRect {
        position: PhysicalPosition::new(x, y),
        size: PhysicalSize::new(width, height),
    }
}
fn anchor(x: i32, y: i32, width: u32, height: u32) -> Option<Rect> {
    Some(Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(width, height).into(),
    })
}

#[test]
fn panel_uses_logical_width_and_stays_inside_retina_and_negative_origin_monitors() {
    let (size, position) = placement(
        &area(0, 46, 2880, 1754),
        2.0,
        anchor(2500, 0, 44, 44),
        360.0,
    );
    assert_eq!(size, LogicalSize::new(360.0, 360.0));
    assert_eq!(position, PhysicalPosition::new(2160, 52));
    let (_, position) = placement(
        &area(-1920, 24, 1920, 1056),
        1.0,
        anchor(-1900, 0, 22, 24),
        360.0,
    );
    assert_eq!(position, PhysicalPosition::new(-1920, 28));
}

#[test]
fn bottom_taskbars_open_upward_and_tiny_work_areas_constrain_the_window() {
    let (_, position) = placement(
        &area(0, 0, 1920, 1040),
        1.0,
        anchor(1000, 1040, 24, 40),
        360.0,
    );
    assert_eq!(position, PhysicalPosition::new(832, 676));
    let (size, position) = placement(&area(-200, 0, 200, 180), 1.0, anchor(-20, 0, 24, 24), 360.0);
    assert_eq!(size, LogicalSize::new(200.0, 180.0));
    assert_eq!(position, PhysicalPosition::new(-200, 0));
}

#[test]
fn content_height_replaces_the_square_window_and_keeps_the_tray_gap() {
    let (size, position) = placement(
        &area(0, 0, 1920, 1040),
        1.0,
        anchor(1000, 1040, 24, 40),
        265.0,
    );
    assert_eq!(size, LogicalSize::new(360.0, 265.0));
    assert_eq!(position, PhysicalPosition::new(832, 771));
    let (size, position) = placement(
        &area(0, 46, 2880, 1754),
        2.0,
        anchor(2500, 0, 44, 44),
        312.0,
    );
    assert_eq!(size, LogicalSize::new(360.0, 312.0));
    assert_eq!(position, PhysicalPosition::new(2160, 52));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_retina_anchor_and_window_position_share_global_logical_coordinates() {
    let tray = logical_tray_anchor(anchor(2664, 0, 44, 48).unwrap(), 2.0);
    assert_eq!(
        tray.position,
        tauri::LogicalPosition::new(1332.0, 0.0).into()
    );
    // 面板最初可能在 1x 屏幕上，最终位置必须使用目标 2x 屏幕的逻辑点。
    for height in [265.0, 340.0] {
        let (_, position) = placement(&area(0, 48, 3840, 2080), 2.0, Some(tray), height);
        assert_eq!(
            window_position(position, 2.0),
            tauri::LogicalPosition::new(1163.0, 28.0).into()
        );
    }
    let tray = logical_tray_anchor(anchor(-2400, 0, 44, 48).unwrap(), 2.0);
    let (_, position) = placement(&area(-3840, 48, 3840, 2080), 2.0, Some(tray), 265.0);
    assert_eq!(
        window_position(position, 2.0),
        tauri::LogicalPosition::new(-1369.0, 28.0).into()
    );
}
