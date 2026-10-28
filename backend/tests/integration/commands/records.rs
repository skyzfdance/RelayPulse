use crate::{
    commands::register,
    desktop::directories::{DirectoryDesktop, DirectoryOpener, OpenDirectoryError},
    shared::{
        model_client::test_support::{Response, Server},
        persistence::test_support::environment,
    },
    startup,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

#[derive(Default)]
struct Desktop {
    opened: Mutex<Vec<PathBuf>>,
    fail: AtomicBool,
}
impl DirectoryOpener for Desktop {
    fn open(&self, path: &Path) -> Result<(), OpenDirectoryError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(OpenDirectoryError);
        }
        self.opened.lock().unwrap().push(path.into());
        Ok(())
    }
}
fn app(root: &Path, desktop: Arc<Desktop>) -> tauri::App<MockRuntime> {
    register(mock_builder())
        .manage(startup::initialize(Ok(environment(root, "com.relaypulse.app.dev"))).0)
        .manage(DirectoryDesktop(desktop))
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
    .map(|b| b.deserialize().unwrap())
}
fn query() -> Value {
    json!({"range":"all","siteId":null,"modelName":null,"result":null,"page":1,"pageSize":10})
}
fn token_query() -> Value {
    json!({"range":"all","siteId":null,"page":1,"pageSize":10})
}
fn exact_keys(value: &Value, keys: &[&str]) {
    let mut actual = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut expected = keys.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}
#[test]
fn all_nine_commands_use_real_registered_ipc_http_files_and_history_without_changing_health() {
    let temp = tempfile::tempdir().unwrap();
    let desktop = Arc::new(Desktop::default());
    let mut success=Response::sse(&[
        ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n",0),
        ("data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":0,\"output_tokens\":2,\"total_tokens\":2}}}\n\n",10),
    ]);
    success.headers = vec![
        ("x-repeat", "one".into()),
        ("x-repeat", "two".into()),
        ("set-cookie", "session=private-cookie".into()),
        ("x-future-header", "kept".into()),
        ("x-request-id", "provider-id".into()),
    ];
    let failed = json!({"error":{"message":"unavailable"}});
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"remote"}]})),
        success,
        Response::raw(503, "application/json", failed.to_string()),
    ]);
    let app = app(temp.path(), desktop.clone());
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let id=invoke(&window,"create_site",json!({"input":{"name":"original","protocol":"openai-responses","baseUrl":server.url,"secret":"ipc-record-secret","enabled":false}})).unwrap();
    let site = invoke(&window, "get_site", json!({"siteId":id})).unwrap();
    let mut connection = site["connection"].clone();
    connection["headers"] = json!([
        {"key":"X-Duplicate","value":"last"},
        {"key":"user-agent","value":"custom-agent"},
        {"key":"x-access-token","value":"independent-token"},{"key":"x-note","value":"safe"}
    ]);
    invoke(
        &window,
        "update_site_connection",
        json!({"siteId":id,"connection":connection}),
    )
    .unwrap();
    invoke(&window, "run_site_probe", json!({"siteId":id})).unwrap();
    let options = invoke(&window, "get_probe_record_options", json!({})).unwrap();
    assert_eq!(
        options,
        json!({"sites":[{"id":id,"name":"original"}],"models":[{"name":"remote","siteIds":[id]}]})
    );
    let records = invoke(&window, "get_probe_records", json!({"query":query()})).unwrap();
    exact_keys(&records, &["items", "total", "page", "retainedCount"]);
    assert_eq!(
        (records["total"].clone(), records["retainedCount"].clone()),
        (json!(1), json!(1))
    );
    let first = &records["items"][0];
    exact_keys(
        first,
        &[
            "id",
            "siteName",
            "modelName",
            "probedAt",
            "result",
            "resultMessage",
            "firstTokenMs",
            "durationMs",
            "totalTokens",
        ],
    );
    assert_eq!(first["result"], "success");
    assert_eq!(first["totalTokens"], 2);
    let record_id = first["id"].clone();
    let detail = invoke(
        &window,
        "get_probe_record_detail",
        json!({"recordId":record_id}),
    )
    .unwrap();
    assert!(detail.get("totalTokens").is_none());
    assert_eq!(detail["usage"]["inputTokens"], 0);
    assert_eq!(detail["usageSource"], "provider");
    assert_eq!(detail["requestInfo"]["strategyLabel"], "低推理");
    assert_eq!(detail["requestInfo"]["requestId"], "provider-id");
    let request = invoke(
        &window,
        "get_probe_record_request",
        json!({"recordId":record_id}),
    )
    .unwrap();
    exact_keys(&request, &["method", "bodyBytes", "headers", "body"]);
    assert_eq!(
        request["bodyBytes"],
        request["body"].as_str().unwrap().len()
    );
    assert_eq!(
        detail["requestSummary"],
        json!({"method":request["method"],"bodyBytes":request["bodyBytes"]})
    );
    let headers = request["headers"].as_array().unwrap();
    for (name, value) in [
        ("authorization", "[REDACTED]"),
        ("x-access-token", "[REDACTED]"),
        ("user-agent", "custom-agent"),
        ("content-type", "application/json"),
        ("accept", "*/*"),
        ("x-duplicate", "last"),
        ("x-note", "safe"),
    ] {
        assert!(
            headers.contains(&json!({"name":name,"value":value})),
            "{name}: {headers:?}"
        );
    }
    assert_eq!(
        headers
            .iter()
            .filter(|h| h["name"] == "x-duplicate")
            .count(),
        1
    );
    let response = invoke(
        &window,
        "get_probe_record_response",
        json!({"recordId":record_id}),
    )
    .unwrap();
    exact_keys(
        &response,
        &[
            "state",
            "kind",
            "receivedBytes",
            "headers",
            "chunkCount",
            "chunks",
            "errorResponse",
            "logFile",
        ],
    );
    assert_eq!(response["state"], "received");
    assert_eq!(response["kind"], "stream");
    assert!(response["errorResponse"].is_null());
    assert_eq!(
        detail["responseSummary"],
        json!({"state":response["state"],"kind":response["kind"],"receivedBytes":response["receivedBytes"]})
    );
    let repeated = response["headers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["name"] == "x-repeat")
        .map(|h| h["value"].clone())
        .collect::<Vec<_>>();
    assert_eq!(repeated, vec![json!("one"), json!("two")]);
    assert!(response["headers"]
        .as_array()
        .unwrap()
        .contains(&json!({"name":"set-cookie","value":"[REDACTED]"})));
    let path = PathBuf::from(response["logFile"]["path"].as_str().unwrap());
    assert!(path.is_absolute());
    assert_eq!(path.file_name().unwrap(), "response.jsonl");
    let text = fs::read_to_string(&path).unwrap();
    for secret in [
        "ipc-record-secret",
        "independent-token",
        "private-cookie",
        "must-be-overridden",
    ] {
        assert!(!text.contains(secret));
    }
    let events = text
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    let chunks = events
        .iter()
        .filter(|e| e["event"] == "response_chunk")
        .collect::<Vec<_>>();
    assert_eq!(response["chunkCount"], chunks.len());
    assert_eq!(
        response["receivedBytes"],
        chunks
            .iter()
            .map(|e| e["receivedBytes"].as_u64().unwrap())
            .sum::<u64>()
    );
    assert_ne!(
        response["receivedBytes"].as_u64().unwrap(),
        fs::metadata(&path).unwrap().len()
    );
    assert_eq!(
        invoke(
            &window,
            "open_probe_record_log_directory",
            json!({"recordId":record_id})
        )
        .unwrap(),
        json!({"message":"已打开探测日志目录。"})
    );
    assert_eq!(
        *desktop.opened.lock().unwrap(),
        vec![path.parent().unwrap().to_owned()]
    );
    desktop.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        invoke(
            &window,
            "open_probe_record_log_directory",
            json!({"recordId":record_id})
        )
        .unwrap_err()["code"],
        "probe_record_log_open_failed"
    );
    desktop.fail.store(false, Ordering::SeqCst);
    invoke(&window, "run_site_probe", json!({"siteId":id})).unwrap();
    let health = invoke(&window, "get_site", json!({"siteId":id})).unwrap();
    let mut q = query();
    q["result"] = json!("failure");
    let failures = invoke(&window, "get_probe_records", json!({"query":q})).unwrap();
    assert_eq!(failures["total"], 1);
    assert_eq!(failures["retainedCount"], 2);
    let failed_id = failures["items"][0]["id"].clone();
    let error = invoke(
        &window,
        "get_probe_record_response",
        json!({"recordId":failed_id}),
    )
    .unwrap();
    assert_eq!(
        error["errorResponse"],
        json!({"body":failed.to_string(),"bodyComplete":true})
    );
    assert_eq!(error["chunkCount"], Value::Null);
    let stats = invoke(
        &window,
        "get_probe_token_stats",
        json!({"query":token_query()}),
    )
    .unwrap();
    assert_eq!(
        stats["summary"],
        json!({"probeCount":2,"reportedCount":1,"totalReportedCount":1,"inputTokens":0,"outputTokens":2,"totalTokens":2,"averageTokens":2.0})
    );
    assert_eq!(
        invoke(
            &window,
            "delete_probe_record",
            json!({"recordId":record_id})
        )
        .unwrap(),
        Value::Null
    );
    assert!(!path.parent().unwrap().exists());
    assert_eq!(
        invoke(&window, "get_site", json!({"siteId":id})).unwrap(),
        health
    );
    let stats = invoke(
        &window,
        "get_probe_token_stats",
        json!({"query":token_query()}),
    )
    .unwrap();
    assert_eq!(stats["summary"]["probeCount"], 1);
    assert_eq!(stats["summary"]["totalTokens"], Value::Null);
    let iq = temp
        .path()
        .join("com.relaypulse.app.dev/iq-test/run/stream.sse");
    fs::create_dir_all(iq.parent().unwrap()).unwrap();
    fs::write(&iq, b"IQ sentinel").unwrap();
    assert_eq!(
        invoke(&window, "clear_probe_records", json!({})).unwrap(),
        Value::Null
    );
    assert!(!Path::new(error["logFile"]["path"].as_str().unwrap()).exists());
    assert_eq!(fs::read(iq).unwrap(), b"IQ sentinel");
    assert_eq!(
        invoke(&window, "get_probe_record_options", json!({})).unwrap(),
        options
    );
    assert_eq!(
        invoke(&window, "get_site", json!({"siteId":id})).unwrap(),
        health
    );
    let empty = invoke(
        &window,
        "get_probe_token_stats",
        json!({"query":token_query()}),
    )
    .unwrap();
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty["retainedCount"], 0);
    assert_eq!(empty["summary"]["probeCount"], 0);
    assert!(empty["summary"]["totalTokens"].is_null());
    assert_eq!(
        invoke(&window, "clear_probe_records", json!({})).unwrap(),
        Value::Null
    );
}
#[test]
fn ipc_rejects_malformed_missing_unknown_and_unsafe_values_with_exact_error_fields() {
    let temp = tempfile::tempdir().unwrap();
    let app = app(temp.path(), Arc::new(Desktop::default()));
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (key, value, field) in [
        ("range", json!("future"), "query.range"),
        ("siteId", json!("  "), "query.siteId"),
        ("siteId", json!("x".repeat(201)), "query.siteId"),
        ("modelName", json!(""), "query.modelName"),
        ("result", json!("succeeded"), "query.result"),
        ("page", json!(0), "query.page"),
        ("page", json!(-1), "query.page"),
        ("page", json!(1.5), "query.page"),
        ("page", json!(9_007_199_254_740_992u64), "query.page"),
        ("pageSize", json!(15), "query.pageSize"),
        ("siteId", json!(false), "query.siteId"),
        ("extra", json!(true), "query"),
    ] {
        let mut q = query();
        q[key] = value;
        let error = invoke(&window, "get_probe_records", json!({"query":q})).unwrap_err();
        assert_eq!(error["code"], "probe_records_validation");
        assert_eq!(error["category"], "validation");
        assert_eq!(error["field"], field);
        assert_eq!(error["retryable"], false);
        assert_eq!(error["operationId"], Value::Null);
    }
    for key in ["range", "siteId", "modelName", "result", "page", "pageSize"] {
        let mut q = query();
        q.as_object_mut().unwrap().remove(key);
        assert_eq!(
            invoke(&window, "get_probe_records", json!({"query":q})).unwrap_err()["field"],
            format!("query.{key}")
        );
    }
    for body in [
        json!({}),
        json!({"query":null}),
        json!({"query":[]}),
        json!({"query":query(),"unknown":1}),
    ] {
        assert_eq!(
            invoke(&window, "get_probe_records", body).unwrap_err()["field"],
            "query"
        );
    }
    let mut q = token_query();
    q["modelName"] = Value::Null;
    assert_eq!(
        invoke(&window, "get_probe_token_stats", json!({"query":q})).unwrap_err()["field"],
        "query"
    );
    for command in [
        "get_probe_record_detail",
        "get_probe_record_request",
        "get_probe_record_response",
        "open_probe_record_log_directory",
        "delete_probe_record",
    ] {
        for body in [
            json!({}),
            json!({"recordId":null}),
            json!({"recordId":42}),
            json!({"recordId":" \n"}),
            json!({"recordId":"missing","path":"/private/input"}),
        ] {
            let error = invoke(&window, command, body).unwrap_err();
            assert_eq!(
                error["code"], "probe_records_validation",
                "{command}: {error}"
            );
            assert_eq!(error["field"], "recordId");
            assert!(!error.to_string().contains("/private/input"));
        }
        let error = invoke(&window, command, json!({"recordId":"../../outside"})).unwrap_err();
        assert_eq!(error["code"], "probe_record_not_found");
        assert_eq!(error["category"], "not_found");
        assert_eq!(error["retryable"], false);
    }
    for command in ["get_probe_record_options", "clear_probe_records"] {
        assert_eq!(
            invoke(&window, command, json!({"query":token_query()})).unwrap_err()["code"],
            "probe_records_validation"
        );
    }
}

#[test]
fn all_nine_commands_report_structured_unavailable_when_startup_is_blocked() {
    let state = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ))
    .0;
    let app = register(mock_builder())
        .manage(state)
        .manage(DirectoryDesktop(Arc::new(Desktop::default())))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (command, body) in [
        ("get_probe_record_options", json!({})),
        ("get_probe_records", json!({"query":query()})),
        ("get_probe_token_stats", json!({"query":token_query()})),
        ("get_probe_record_detail", json!({"recordId":"missing"})),
        ("get_probe_record_request", json!({"recordId":"missing"})),
        ("get_probe_record_response", json!({"recordId":"missing"})),
        (
            "open_probe_record_log_directory",
            json!({"recordId":"missing"}),
        ),
        ("delete_probe_record", json!({"recordId":"missing"})),
        ("clear_probe_records", json!({})),
    ] {
        let error = invoke(&window, command, body).unwrap_err();
        assert_eq!(
            error["code"], "probe_records_unavailable",
            "{command}: {error}"
        );
        assert_eq!(error["category"], "unavailable");
        assert_eq!(error["retryable"], true);
        assert!(error["field"].is_null());
        assert!(error["operationId"].is_null());
    }
}
