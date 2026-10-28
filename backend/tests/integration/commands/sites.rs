use crate::{
    commands::register,
    shared::model_client::test_support::{Response, Server},
    shared::persistence::test_support::environment,
    startup,
};
use serde_json::{json, Value};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};
fn app(root: &std::path::Path) -> tauri::App<MockRuntime> {
    register(mock_builder())
        .manage(startup::initialize(Ok(environment(root, "com.relaypulse.app.dev"))).0)
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
fn all_seventeen_commands_use_registered_ipc_real_http_and_persistence() {
    let tmp = tempfile::tempdir().unwrap();
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"a"},{"id":"b"}]})),
        Response::json(json!(["a", "c"])),
        Response::raw(503, "text/plain", "upstream error"),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let app = app(tmp.path());
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let protocols = invoke(&window, "get_site_protocols", json!({})).unwrap();
    assert_eq!(protocols.as_array().unwrap().len(), 4);
    assert_eq!(
        protocols[0],
        json!({"value":"openai-compatible","label":"OpenAI 兼容协议"})
    );
    assert_eq!(invoke(&window, "get_sites", json!({})).unwrap(), json!([]));
    let id=invoke(&window,"create_site",json!({"input":{"name":"local","protocol":"custom-http","baseUrl":server.url,"secret":"ipc-private-key","enabled":true}})).unwrap();
    assert!(id.is_string());
    let site = invoke(&window, "get_site", json!({"siteId":id})).unwrap();
    assert_eq!(site.as_object().unwrap().len(), 9);
    assert_eq!(site["models"][0].as_object().unwrap().len(), 8);
    assert!(!site.to_string().contains("ipc-private-key"));
    assert_eq!(site["credentialConfigured"], true);
    assert_eq!(site["models"][0]["health"], "muted");
    assert_ne!(site["models"][0]["id"], site["models"][0]["name"]);
    let a = site["models"][0]["id"].clone();
    let b = site["models"][1]["id"].clone();
    let mut connection = site["connection"].clone();
    connection["intervalSeconds"] = json!(60);
    connection["headers"] = json!([{"key":"X-Empty","value":""}]);
    invoke(
        &window,
        "update_site_connection",
        json!({"siteId":id,"connection":connection}),
    )
    .unwrap();
    let saved=invoke(&window,"save_site",json!({"siteId":id,"draft":{"name":"saved","protocol":"custom-http","baseUrl":server.url,"credentialAction":"keep","secret":"ignored"}})).unwrap();
    assert_eq!(saved["connection"], connection);
    let enabled = invoke(
        &window,
        "set_site_enabled",
        json!({"siteId":id,"enabled":false}),
    )
    .unwrap();
    assert_eq!(enabled.as_object().unwrap().len(), 7);
    assert_eq!(enabled["enabled"], false);
    assert_eq!(
        invoke(&window, "reorder_sites", json!({"ids":[id]})).unwrap()[0]["priority"],
        1
    );
    let added = invoke(
        &window,
        "add_site_model",
        json!({"siteId":id,"name":"manual"}),
    )
    .unwrap();
    let manual = added["models"][2]["id"].clone();
    invoke(
        &window,
        "delete_site_model",
        json!({"siteId":id,"modelId":manual}),
    )
    .unwrap();
    invoke(
        &window,
        "set_site_model_enabled",
        json!({"siteId":id,"modelId":a,"enabled":false}),
    )
    .unwrap();
    let before = invoke(&window, "get_site", json!({"siteId":id})).unwrap();
    let changes = invoke(
        &window,
        "sync_site_models",
        json!({"siteId":id,"models":["a","b"]}),
    )
    .unwrap();
    assert_eq!(
        changes,
        json!({"added":["c"],"removed":[{"id":b,"name":"b"}]})
    );
    assert_eq!(
        invoke(&window, "get_site", json!({"siteId":id})).unwrap(),
        before
    );
    assert_eq!(
        invoke(
            &window,
            "apply_site_model_sync",
            json!({"siteId":id,"models":["a","b"],"added":["c"],"removed":[b]})
        )
        .unwrap(),
        Value::Null
    );
    let custom =
        json!({"endpoint":"/specific","parameters":[{"key":"n","type":"number","value":"2"}]});
    invoke(
        &window,
        "update_site_probe_policy",
        json!({"siteId":id,"modelId":a,"policy":null,"customRequest":custom}),
    )
    .unwrap();
    let trial = invoke(
        &window,
        "test_model_probe",
        json!({"siteId":id,"modelId":a,"request":{"endpoint":"/draft","parameters":[]}}),
    )
    .unwrap();
    assert_eq!(trial.as_object().unwrap().len(), 5);
    assert_eq!(trial["success"], false);
    assert_eq!(trial["simulated"], false);
    assert_eq!(trial["statusCode"], 503);
    let single = invoke(&window, "run_site_probe", json!({"siteId":id,"modelId":a})).unwrap();
    assert_eq!(single["models"][0]["health"], "healthy");
    assert_eq!(single["models"][0]["firstTokenMs"], Value::Null);
    assert_eq!(single["models"][1]["health"], "muted");
    let batch = invoke(&window, "run_site_probe", json!({"siteId":id})).unwrap();
    assert_eq!(batch["models"][1]["health"], "healthy");
    assert_eq!(batch["models"][0], single["models"][0]);
    let summary = invoke(&window, "get_sites", json!({})).unwrap();
    assert_eq!(summary[0]["health"], "healthy");
    assert_eq!(summary[0]["modelCount"], 2);
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert!(requests[2].head.starts_with("POST /draft"));
    assert!(requests[3].head.starts_with("POST /specific"));
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[3].body).unwrap()["n"].as_u64(),
        Some(2)
    );
    assert!(requests.iter().all(|r| r
        .head
        .to_lowercase()
        .contains("authorization: bearer ipc-private-key")));
    drop(window);
    drop(app);
    let reopened = self::app(tmp.path());
    let window = WebviewWindowBuilder::new(&reopened, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&window, "get_site", json!({"siteId":id})).unwrap(),
        batch
    );
    assert_eq!(
        invoke(&window, "delete_site", json!({"siteId":id})).unwrap(),
        Value::Null
    );
    assert_eq!(invoke(&window, "get_sites", json!({})).unwrap(), json!([]));
}
#[test]
fn malformed_and_missing_nullable_fields_are_structured_and_do_not_leak_inputs() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path());
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (command, body) in [
        ("get_site", json!({})),
        ("get_site", json!({"siteId":1})),
        ("get_site", json!({"siteId":"x","unknown":true})),
        (
            "create_site",
            json!({"input":{"name":"a","protocol":"invalid","baseUrl":"http://localhost","secret":"must-not-leak","enabled":true}}),
        ),
        (
            "apply_site_model_sync",
            json!({"siteId":"x","models":[],"added":[]}),
        ),
        (
            "update_site_probe_policy",
            json!({"siteId":"x","modelId":null,"policy":null}),
        ),
        (
            "update_site_probe_policy",
            json!({"siteId":"x","policy":null,"customRequest":null}),
        ),
        (
            "update_site_probe_policy",
            json!({"siteId":"x","modelId":null,"customRequest":null}),
        ),
        ("set_site_enabled", json!({"siteId":"x","enabled":1})),
    ] {
        let error = invoke(&window, command, body).unwrap_err();
        assert_eq!(error["code"], "invalid_input", "{command}: {error}");
        assert!(!error.to_string().contains("must-not-leak"));
        assert!(error["retryable"].is_boolean());
    }
    let missing = invoke(&window, "get_site", json!({"siteId":"missing"})).unwrap_err();
    assert_eq!(missing["code"], "site_not_found");
    for command in ["preview_site_probe_policy", "preview_custom_probe_request"] {
        assert!(invoke(&window, command, json!({})).is_err());
    }
}
