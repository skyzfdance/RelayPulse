use crate::{commands::register, shared::persistence::test_support::environment, startup};
use serde_json::{json, Value};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};
fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
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
fn query() -> Value {
    json!({"startDate":null,"endDate":null,"source":[],"result":[],"actions":[],"page":1,"pageSize":10})
}
#[test]
fn real_registered_commands_validate_strict_boundary_and_return_structured_errors() {
    let temp = tempfile::tempdir().unwrap();
    let app = register(mock_builder())
        .manage(startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0)
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let page = invoke(&window, "get_operation_logs", json!({"query":query()})).unwrap();
    assert!(page["items"].is_array());
    assert_eq!(page["page"], 1);
    assert_eq!(page["pageSize"], 10);
    for (key, value, field) in [
        ("startDate", json!("2026-02-29"), "query.startDate"),
        ("endDate", json!(false), "query.endDate"),
        ("page", json!(-1), "query.page"),
        ("page", json!(1.5), "query.page"),
        ("pageSize", json!(70), "query.pageSize"),
        ("source", json!(["panel"]), "query.source"),
        ("result", json!(["skipped"]), "query.result"),
        ("actions", json!(["site_created"]), "query.actions"),
        ("unknown", json!(1), "query"),
    ] {
        let mut q = query();
        q[key] = value;
        let error = invoke(&window, "get_operation_logs", json!({"query":q})).unwrap_err();
        assert_eq!(error["code"], "operation_logs_validation");
        assert_eq!(error["category"], "validation");
        assert_eq!(error["field"], field);
        assert_eq!(error["retryable"], false);
        assert_eq!(error["operationId"], Value::Null);
    }
    for key in [
        "startDate",
        "endDate",
        "source",
        "result",
        "actions",
        "page",
        "pageSize",
    ] {
        let mut q = query();
        q.as_object_mut().unwrap().remove(key);
        assert_eq!(
            invoke(&window, "get_operation_logs", json!({"query":q})).unwrap_err()["field"],
            format!("query.{key}")
        );
    }
    for body in [
        json!({}),
        json!({"query":null}),
        json!({"query":query(),"extra":true}),
    ] {
        assert_eq!(
            invoke(&window, "get_operation_logs", body).unwrap_err()["field"],
            "query"
        );
    }
    let missing = invoke(
        &window,
        "get_operation_log_detail",
        json!({"logId":"missing"}),
    )
    .unwrap_err();
    assert_eq!(missing["code"], "operation_log_not_found");
    assert_eq!(missing["category"], "not_found");
    for body in [
        json!({}),
        json!({"logId":42}),
        json!({"logId":"   "}),
        json!({"logId":"x","extra":true}),
    ] {
        assert_eq!(
            invoke(&window, "get_operation_log_detail", body).unwrap_err()["field"],
            "logId"
        );
    }
    for body in [
        json!({"input":{"target":"route","kind":"page","summary":"forged"}}),
        json!({"input":{"target":"route","kind":"success"}}),
        json!({"input":{"target":"custom","kind":"page"}}),
        json!({"input":{"target":"route","kind":"page"},"extra":true}),
    ] {
        assert_eq!(
            invoke(&window, "report_operation_read_failure", body).unwrap_err()["category"],
            "validation"
        );
    }
    invoke(
        &window,
        "report_operation_read_failure",
        json!({"input":{"target":"route","kind":"page"}}),
    )
    .unwrap();
    invoke(
        &window,
        "report_operation_read_failure",
        json!({"input":{"target":"route","kind":"page"}}),
    )
    .unwrap();
    let mut q = query();
    q["actions"] = json!(["read"]);
    let page = invoke(&window, "get_operation_logs", json!({"query":q})).unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["items"][0]["result"], "failure");
    let detail = invoke(
        &window,
        "get_operation_log_detail",
        json!({"logId":page["items"][0]["id"]}),
    )
    .unwrap();
    assert_eq!(detail["content"]["kind"], "failure");
}
#[test]
fn unavailable_startup_never_falls_back_to_mock() {
    let app = register(mock_builder())
        .manage(
            startup::initialize(Err(
                crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
            ))
            .0,
        )
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (cmd, body) in [
        ("get_operation_logs", json!({"query":query()})),
        ("get_operation_log_detail", json!({"logId":"missing"})),
        (
            "report_operation_read_failure",
            json!({"input":{"target":"route","kind":"page"}}),
        ),
    ] {
        let error = invoke(&window, cmd, body).unwrap_err();
        assert_eq!(error["code"], "operation_logs_unavailable");
        assert_eq!(error["category"], "unavailable");
        assert_eq!(error["retryable"], true);
    }
}
