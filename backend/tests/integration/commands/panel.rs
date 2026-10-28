use crate::panel_test_support as fixture;
use crate::{
    commands::register,
    startup::{self, StartupState},
};
use serde_json::{json, Value};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .map(|body| body.deserialize().unwrap())
}

#[test]
fn both_panel_commands_use_the_actual_registration_and_real_http_monitoring() {
    use crate::shared::model_client::test_support::{Response, Server};
    let f = fixture::Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let id = f.site(&server.url, &["m"], true);
    let fixture::Fixture {
        app: application,
        directory: _directory,
        database,
        store: _store,
    } = f;
    let app = register(mock_builder())
        .manage(StartupState::Ready(application))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let waiting = invoke(&window, "get_panel_snapshot", json!({})).unwrap();
    assert_eq!(waiting["state"], "waiting");
    assert_eq!(waiting["modelCount"], 1);
    assert!(server.requests().is_empty());
    let completed = invoke(&window, "refresh_panel", Value::Null).unwrap();
    assert_eq!(completed["state"], "ready");
    assert_eq!(completed["sites"][0]["siteId"], id);
    assert_eq!(completed["sites"][0]["models"][0]["status"], "healthy");
    assert!(completed["activeProbe"].is_null());
    assert!(completed.get("data").is_none());
    assert_eq!(server.requests().len(), 1);
    assert_eq!(
        invoke(&window, "get_panel_snapshot", json!({})).unwrap(),
        completed
    );
    assert_eq!(
        database
            .with_connection(|c| c
                .query_row("SELECT count(*) FROM probe_records", [], |row| row
                    .get::<_, i64>(0)))
            .unwrap(),
        1
    );
    for command in ["get_panel_snapshot", "refresh_panel"] {
        let error = invoke(&window, command, json!({"siteId":"unexpected"})).unwrap_err();
        assert_eq!(error["code"], "invalid_input");
        assert_eq!(error["field"], "request");
        assert_eq!(error["retryable"], false);
    }
}

#[test]
fn blocked_startup_rejects_both_registered_commands_with_the_shared_error_shape() {
    let state = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ))
    .0;
    let app = register(mock_builder())
        .manage(state)
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for command in ["get_panel_snapshot", "refresh_panel"] {
        let error = invoke(&window, command, json!({})).unwrap_err();
        assert_eq!(
            error,
            json!({"code":"backend_unavailable","category":"startup","message":"小面板服务未就绪，请检查启动状态。","field":null,"retryable":false,"operationId":null})
        );
    }
}

#[test]
fn window_sync_is_registered_and_rejects_other_windows_and_invalid_heights() {
    let app = register(mock_builder())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let error = invoke(
        &main,
        "sync_panel_window",
        json!({"height":265,"theme":"system"}),
    )
    .unwrap_err();
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(error["field"], "window");
    let panel = WebviewWindowBuilder::new(&app, "status-panel", Default::default())
        .build()
        .unwrap();
    for height in [0.0, -1.0] {
        let error = invoke(
            &panel,
            "sync_panel_window",
            json!({"height":height,"theme":"system"}),
        )
        .unwrap_err();
        assert_eq!(error["code"], "invalid_input");
        assert_eq!(error["field"], "height");
    }
}

#[test]
fn opening_settings_is_explicit_and_restricted_to_the_panel_window() {
    let app = register(mock_builder())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(
        &app,
        "main",
        tauri::WebviewUrl::External("http://localhost:1421/config/sites".parse().unwrap()),
    )
    .build()
    .unwrap();
    for command in ["open_panel_settings", "open_panel_sites"] {
        let error = invoke(&main, command, json!({})).unwrap_err();
        assert_eq!(error["field"], "window");
    }
}
