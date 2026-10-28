use super::*;
use crate::{
    commands::register, modules::notifications::tests::FakePlatform,
    shared::persistence::test_support::environment, startup,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

fn app(state: StartupState, platform: Arc<FakePlatform>) -> tauri::App<MockRuntime> {
    register(mock_builder())
        .manage(state)
        .manage(NotificationPlatformState(platform))
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
fn all_nine_commands_route_through_real_registration_and_persist() {
    let temp = tempfile::tempdir().unwrap();
    let platform = Arc::new(FakePlatform::new(NotificationPermission::NotRequested));
    let application = app(
        startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0,
        platform.clone(),
    );
    let window = WebviewWindowBuilder::new(&application, "main", Default::default())
        .build()
        .unwrap();
    let original = invoke(&window, "get_notification_settings", json!({})).unwrap();
    assert_eq!(original.as_object().unwrap().len(), 3);
    assert_eq!(original["authorization"], "not_requested");
    assert_eq!(original["notificationsEnabled"], false);
    assert_eq!(
        original["rules"][0],
        json!({"id":"probe.model_failed", "title":"模型探测故障", "subTitle":"模型达到稳定故障判定时发送系统通知", "status":true, "templateType":"default", "channel":"系统通知"})
    );
    assert_eq!(
        invoke(&window, "request_notification_permission", json!({})).unwrap(),
        "not_requested"
    );
    assert_eq!(*platform.requested.lock().unwrap(), 1);
    *platform.permission.lock().unwrap() = NotificationPermission::Granted;
    let mut input = original;
    input["notificationsEnabled"] = json!(true);
    input["rules"][0]["status"] = json!(false);
    input["authorization"] = json!("denied");
    let saved = invoke(
        &window,
        "update_notification_settings",
        json!({"settings": input}),
    )
    .unwrap();
    assert_eq!(saved["authorization"], "granted");
    assert_eq!(saved["rules"][0]["status"], false);
    let detail = invoke(
        &window,
        "get_notification_template",
        json!({"id":"probe.model_failed"}),
    )
    .unwrap();
    assert_eq!(detail.as_object().unwrap().len(), 12);
    for key in [
        "markerId",
        "titleTemplate",
        "bodyTemplate",
        "defaultMarkerId",
        "defaultTitleTemplate",
        "defaultBodyTemplate",
    ] {
        assert!(detail[key].is_string());
    }
    let update = json!({"templateType":"custom", "markerId":"warning", "titleTemplate":"{{model}} 报警", "bodyTemplate":"{{app}}\n{{endpoint}} {{time}}"});
    let custom = invoke(
        &window,
        "update_notification_template",
        json!({"id":"probe.model_failed", "update":update}),
    )
    .unwrap();
    assert_eq!(custom["templateType"], "custom");
    assert_eq!(custom["titleTemplate"], update["titleTemplate"]);
    let markers = invoke(&window, "get_notification_markers", json!({})).unwrap();
    assert_eq!(markers.as_array().unwrap().len(), 7);
    assert_eq!(
        markers[0],
        json!({"markerId":"none", "symbol":"", "name":"无标记", "tone":"neutral"})
    );
    let variables = invoke(&window, "get_notification_variables", json!({})).unwrap();
    assert_eq!(
        variables
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["time", "app", "model", "endpoint"]
    );
    assert_eq!(
        invoke(
            &window,
            "send_test_notification",
            json!({"template":{"title":"⚠️ 成品", "body":"正文", "markerId":"warning"}})
        )
        .unwrap(),
        Value::Null
    );
    assert_eq!(
        *platform.sent.lock().unwrap(),
        vec![("⚠️ 成品".into(), "正文".into())]
    );
    assert_eq!(
        invoke(&window, "open_notification_settings", json!({})).unwrap(),
        Value::Null
    );
    assert_eq!(*platform.opened.lock().unwrap(), 1);
    drop(window);
    drop(application);
    let application = app(
        startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0,
        platform,
    );
    let window = WebviewWindowBuilder::new(&application, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(
            &window,
            "get_notification_template",
            json!({"id":"probe.model_failed"})
        )
        .unwrap(),
        custom
    );
    let reopened = invoke(&window, "get_notification_settings", json!({})).unwrap();
    assert_eq!(reopened["notificationsEnabled"], true);
    assert_eq!(reopened["rules"][0]["status"], false);
    assert_eq!(reopened["rules"][0]["templateType"], "custom");
}

fn assert_error(error: &Value, code: &str) {
    assert_eq!(error.as_object().unwrap().len(), 6);
    assert_eq!(error["code"], code);
    assert!(error["category"].is_string());
    assert!(!error["message"].as_str().unwrap().is_empty());
    assert!(error["retryable"].is_boolean());
    assert!(error["operationId"].is_null());
    assert!(!error["message"].as_str().unwrap().contains("secret-path"));
}

#[test]
fn malformed_missing_fields_and_domain_errors_are_structured() {
    let temp = tempfile::tempdir().unwrap();
    let platform = Arc::new(FakePlatform::new(NotificationPermission::Denied));
    let application = app(
        startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0,
        platform.clone(),
    );
    let window = WebviewWindowBuilder::new(&application, "main", Default::default())
        .build()
        .unwrap();
    for (command, body, code) in [
        ("update_notification_settings", json!({}), "invalid_input"),
        (
            "update_notification_settings",
            json!({"settings":null}),
            "invalid_input",
        ),
        ("get_notification_template", json!({}), "invalid_input"),
        (
            "get_notification_template",
            json!({"id":false}),
            "invalid_input",
        ),
        (
            "get_notification_template",
            json!({"id":"/secret-path"}),
            "notification_rule_not_found",
        ),
        (
            "update_notification_template",
            json!({"id":"probe.model_failed"}),
            "invalid_input",
        ),
        (
            "update_notification_template",
            json!({"id":"probe.model_failed","update":{"templateType":"custom","markerId":"none","titleTemplate":"{{secret}}","bodyTemplate":"body"}}),
            "invalid_input",
        ),
        ("send_test_notification", json!({}), "invalid_input"),
        (
            "send_test_notification",
            json!({"template":{"title":"hello","body":"body"}}),
            "notifications_disabled",
        ),
    ] {
        assert_error(&invoke(&window, command, body).unwrap_err(), code);
    }
    let mut input = invoke(&window, "get_notification_settings", json!({})).unwrap();
    input["notificationsEnabled"] = json!(true);
    invoke(
        &window,
        "update_notification_settings",
        json!({"settings":input}),
    )
    .unwrap();
    assert_error(
        &invoke(
            &window,
            "send_test_notification",
            json!({"template":{"title":"hello","body":"body"}}),
        )
        .unwrap_err(),
        "notification_permission_denied",
    );
    for (operation, command, body, code) in [
        (
            "permission_query",
            "get_notification_settings",
            json!({}),
            "notification_permission_query_failed",
        ),
        (
            "permission_request",
            "request_notification_permission",
            json!({}),
            "notification_permission_request_failed",
        ),
        (
            "open_settings",
            "open_notification_settings",
            json!({}),
            "notification_settings_open_failed",
        ),
        (
            "send",
            "send_test_notification",
            json!({"template":{"title":"hello","body":"body"}}),
            "notification_send_failed",
        ),
    ] {
        *platform.permission.lock().unwrap() = NotificationPermission::Granted;
        *platform.fail.lock().unwrap() = Some(operation);
        assert_error(&invoke(&window, command, body).unwrap_err(), code);
    }
}

#[test]
fn blocked_startup_does_not_expose_paths_or_underlying_errors() {
    let state = StartupState::Blocked(startup::StartupError::Persistence(
        crate::shared::persistence::PersistenceError::PathConflict("/secret-path/database".into()),
    ));
    let application = app(
        state,
        Arc::new(FakePlatform::new(NotificationPermission::Granted)),
    );
    let window = WebviewWindowBuilder::new(&application, "main", Default::default())
        .build()
        .unwrap();
    for (command, body) in [
        ("get_notification_settings", json!({})),
        (
            "get_notification_template",
            json!({"id":"probe.model_failed"}),
        ),
        ("request_notification_permission", json!({})),
        (
            "send_test_notification",
            json!({"template":{"title":"hello","body":"body"}}),
        ),
        ("open_notification_settings", json!({})),
    ] {
        assert_error(
            &invoke(&window, command, body).unwrap_err(),
            "backend_unavailable",
        );
    }
}

#[test]
fn storage_errors_do_not_expose_underlying_database_details() {
    let error: CommandError = NotificationError::Storage(
        crate::shared::persistence::PersistenceError::PathConflict("/secret-path/database".into()),
    )
    .into();
    assert_error(
        &serde_json::to_value(error).unwrap(),
        "database_unavailable",
    );
}
