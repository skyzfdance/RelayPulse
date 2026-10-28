use super::*;
use crate::{commands::register, shared::persistence::test_support::environment, startup};
use serde_json::{json, Value};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

fn app(state: StartupState) -> tauri::App<MockRuntime> {
    register(mock_builder())
        .manage(state)
        .build(mock_context(noop_assets()))
        .unwrap()
}

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
fn settings_round_trip_through_registered_ipc_and_survive_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let state = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    let app = app(state);
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let original = invoke(&window, "get_general_settings", json!({})).unwrap();
    assert_eq!(
        original,
        json!({
            "launchAtStartup": false, "continueInBackground": true, "showTrayIcon": true,
            "theme": "system", "language": "zh-CN", "historyRetentionDays": 30,
        })
    );
    let saved = json!({
        "launchAtStartup": true, "continueInBackground": false, "showTrayIcon": false,
        "theme": "dark", "language": "zh-CN", "historyRetentionDays": 3650,
    });
    assert_eq!(
        invoke(
            &window,
            "update_general_settings",
            json!({"settings": saved})
        )
        .unwrap(),
        saved
    );
    assert_eq!(
        invoke(&window, "get_general_settings", json!({})).unwrap(),
        saved
    );
    drop(window);
    drop(app);
    let reopened = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    assert_eq!(
        serde_json::to_value(configuration(&reopened).unwrap().load().unwrap()).unwrap(),
        saved
    );
}

#[test]
fn invalid_ipc_settings_are_rejected_without_changing_persisted_values() {
    let temp = tempfile::tempdir().unwrap();
    let app = app(startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0);
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let original = invoke(&window, "get_general_settings", json!({})).unwrap();
    for (field, value) in [
        ("historyRetentionDays", json!(0)),
        ("historyRetentionDays", json!(3651)),
        ("launchAtStartup", json!("true")),
        ("theme", json!("blue")),
        ("language", json!("en-US")),
        ("unknown", json!(true)),
    ] {
        let mut settings = original.clone();
        settings[field] = value;
        let error = invoke(
            &window,
            "update_general_settings",
            json!({"settings": settings}),
        )
        .unwrap_err();
        assert_eq!(error["code"], "invalid_input");
        assert_eq!(error["retryable"], false);
    }
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("theme");
    assert_eq!(
        invoke(
            &window,
            "update_general_settings",
            json!({"settings": missing})
        )
        .unwrap_err()["code"],
        "invalid_input"
    );
    assert_eq!(
        invoke(&window, "get_general_settings", json!({})).unwrap(),
        original
    );
    // 未迁移的日志命令未注册，不能伪造成功数据。
    assert!(invoke(&window, "get_log_statistics", json!({})).is_err());
}

#[test]
fn blocked_startup_is_reported_by_both_commands() {
    let state = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ))
    .0;
    let app = app(state);
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (command, body) in [
        ("get_general_settings", json!({})),
        ("update_general_settings", json!({"settings": {}})),
    ] {
        let error = invoke(&window, command, body).unwrap_err();
        assert_eq!(error["code"], "backend_unavailable");
        assert_eq!(error["category"], "startup");
        assert!(error["message"]
            .as_str()
            .unwrap()
            .contains("environment initialization blocked"));
    }
}
