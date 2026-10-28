use super::*;

#[test]
fn single_and_double_clicks_toggle_once_and_never_request_the_main_window() {
    let mut state = Presentation::default();
    let now = Instant::now();
    let interval = Duration::from_millis(500);
    assert_eq!(state.tray_click(now, interval), Some(true));
    state.show_requested = true;
    // 双击第二下只保持已打开的面板，不重复关闭。
    assert_eq!(
        state.tray_click(now + Duration::from_millis(100), interval),
        Some(false)
    );
    // 下一次独立点击仍可以关闭；双击关闭不重新打开。
    assert_eq!(
        state.tray_click(now + Duration::from_secs(1), interval),
        Some(true)
    );
    state.show_requested = false;
    assert_eq!(
        state.tray_click(now + Duration::from_millis(1100), interval),
        None
    );
}

#[test]
fn outside_focus_loss_closes_but_tray_press_does_not_erase_the_toggle_intent() {
    let mut state = Presentation {
        show_requested: true,
        tray_pressed: true,
        ..Default::default()
    };
    assert!(!state.dismiss_on_blur(false));
    assert!(state.show_requested);
    state.tray_pressed = false;
    // 原生焦点通知可能早于鼠标按下回调，使用原生事件与图标位置补齐。
    assert!(!state.dismiss_on_blur(true));
    assert!(state.show_requested);
    assert!(state.dismiss_on_blur(false));
    assert!(!state.show_requested);
    assert!(!state.dismiss_on_blur(false));
    assert_eq!(
        state.tray_click(Instant::now(), Duration::from_millis(500)),
        Some(true)
    );
}
