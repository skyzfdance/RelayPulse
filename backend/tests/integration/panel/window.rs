use super::*;
use std::sync::{Arc, Mutex};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Listener;

fn navigation_events(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    window.listen(super::super::main_window::NAVIGATE, move |event| {
        captured
            .lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    events
}

#[test]
fn tray_window_reuses_the_native_panel_route_without_recreating_the_webview() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    open_with(app.handle(), None, true, |_, _| {
        panic!("内容未就绪时不能显示窗口")
    })
    .unwrap();
    let window = app.get_webview_window(WINDOW_LABEL).unwrap();
    assert_eq!(window.url().unwrap().path(), "/panel");
    assert!(
        app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
    assert_eq!(app.webview_windows().len(), 1);
    // MockRuntime 的 is_visible 恒为 true，显示/隐藏的视觉效果留给原生冒烟。
    app.state::<PanelWindow>()
        .presentation
        .lock()
        .unwrap()
        .content_height = Some(265.0);
    open_with(app.handle(), None, false, |_, height| {
        assert_eq!(height, 265.0);
        // 原生显示/隐藏会同步触发焦点通知，回调必须能够访问展示状态。
        assert!(app.state::<PanelWindow>().presentation.try_lock().is_ok());
        Ok(())
    })
    .unwrap();
    assert_eq!(app.webview_windows().len(), 1);
    hide(app.handle()).unwrap();
    assert_eq!(app.webview_windows().len(), 1);
    assert!(
        !app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
}

#[test]
fn cancelling_the_first_open_before_content_arrives_keeps_the_panel_hidden() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    open_with(app.handle(), None, true, |_, _| panic!("尚无内容")).unwrap();
    open_with(app.handle(), None, true, |_, _| panic!("已取消打开")).unwrap();
    assert!(
        !app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
    assert!(app
        .state::<PanelWindow>()
        .presentation
        .lock()
        .unwrap()
        .content_height
        .is_none());
    assert_eq!(app.webview_windows().len(), 1);
}

#[test]
fn explicit_settings_action_reuses_main_and_cancels_panel_show_intent() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::External("http://localhost:1421/config/sites".parse().unwrap()),
    )
    .build()
    .unwrap();
    open_with(app.handle(), None, true, |_, _| panic!("尚无内容")).unwrap();
    let events = navigation_events(&main);
    super::super::main_window::settings(app.handle()).unwrap();
    assert_eq!(main.url().unwrap().path(), "/config/sites");
    assert_eq!(*events.lock().unwrap(), ["/config/general"]);
    assert_eq!(app.webview_windows().len(), 2);
    assert!(
        !app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
}

#[test]
fn explicit_application_open_recreates_a_closed_main_without_opening_the_panel() {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = vec![tauri::utils::config::WindowConfig {
        label: "main".into(),
        ..Default::default()
    }];
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(context)
        .unwrap();
    super::super::main_window::settings(app.handle()).unwrap();
    let main = app.get_webview_window("main").unwrap();
    assert_eq!(main.url().unwrap().path(), "/config/general");
    assert!(app.get_webview_window(WINDOW_LABEL).is_none());
}

#[test]
fn view_all_reuses_main_navigates_to_sites_and_hides_the_panel() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::External("http://localhost:1421/config/general".parse().unwrap()),
    )
    .build()
    .unwrap();
    open_with(app.handle(), None, true, |_, _| panic!("尚无内容")).unwrap();
    let events = navigation_events(&main);
    let panel_events = navigation_events(&app.get_webview_window(WINDOW_LABEL).unwrap());
    super::super::main_window::sites(app.handle()).unwrap();
    assert_eq!(main.url().unwrap().path(), "/config/general");
    assert_eq!(*events.lock().unwrap(), ["/config/sites"]);
    assert!(panel_events.lock().unwrap().is_empty());
    assert_eq!(app.webview_windows().len(), 2);
    assert!(
        !app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
}

#[test]
fn view_all_recreates_a_closed_main_on_sites() {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = vec![tauri::utils::config::WindowConfig {
        label: "main".into(),
        ..Default::default()
    }];
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(context)
        .unwrap();
    super::super::main_window::sites(app.handle()).unwrap();
    assert_eq!(
        app.get_webview_window("main")
            .unwrap()
            .url()
            .unwrap()
            .path(),
        "/config/sites"
    );
    assert!(app.get_webview_window(WINDOW_LABEL).is_none());
}

#[test]
fn notification_click_reuses_main_navigates_to_records_and_hides_panel() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::External("http://localhost:1421/config/general".parse().unwrap()),
    )
    .build()
    .unwrap();
    main.hide().unwrap();
    open_with(app.handle(), None, true, |_, _| panic!("尚无内容")).unwrap();
    let events = navigation_events(&main);
    super::super::main_window::records(app.handle()).unwrap();
    assert_eq!(main.url().unwrap().path(), "/config/general");
    assert_eq!(*events.lock().unwrap(), ["/run/records"]);
    assert_eq!(app.webview_windows().len(), 2);
    assert!(
        !app.state::<PanelWindow>()
            .presentation
            .lock()
            .unwrap()
            .show_requested
    );
}

#[test]
fn repeated_notification_clicks_on_records_request_refresh_without_reloading() {
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let url = "http://localhost:1421/run/records?retained=1"
        .parse()
        .unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::External(url))
        .build()
        .unwrap();
    let events = navigation_events(&main);
    super::super::main_window::records(app.handle()).unwrap();
    super::super::main_window::records(app.handle()).unwrap();
    assert_eq!(main.url().unwrap().query(), Some("retained=1"));
    assert_eq!(*events.lock().unwrap(), ["/run/records", "/run/records"]);
    assert_eq!(app.webview_windows().len(), 1);
}

#[test]
fn notification_click_recreates_a_closed_main_on_records() {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = vec![tauri::utils::config::WindowConfig {
        label: "main".into(),
        ..Default::default()
    }];
    let app = mock_builder()
        .manage(PanelWindow::default())
        .build(context)
        .unwrap();
    super::super::main_window::records(app.handle()).unwrap();
    assert_eq!(
        app.get_webview_window("main")
            .unwrap()
            .url()
            .unwrap()
            .path(),
        "/run/records"
    );
    assert!(app.get_webview_window(WINDOW_LABEL).is_none());
}
