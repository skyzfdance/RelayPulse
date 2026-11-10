use crate::{
    commands::register,
    desktop::directories::{DirectoryDesktop, DirectoryOpener, OpenDirectoryError},
    iq_test_support::{success, Fixture},
    shared::model_client::{
        test_support::{Response, Server},
        Protocol,
    },
    startup::StartupState,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

#[derive(Default)]
struct Desktop(Mutex<Vec<PathBuf>>);
impl DirectoryOpener for Desktop {
    fn open(&self, path: &Path) -> Result<(), OpenDirectoryError> {
        self.0.lock().unwrap().push(path.into());
        Ok(())
    }
}
fn invoke(
    window: &WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
) -> std::result::Result<Value, Value> {
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
    .map(|r| r.deserialize().unwrap())
}
fn keys(value: &Value, expected: &[&str]) {
    let mut actual: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}
fn terminal(window: &WebviewWindow<MockRuntime>, id: &Value) -> Value {
    let start = std::time::Instant::now();
    loop {
        let run = invoke(window, "get_iq_run_details", json!({"id":id})).unwrap();
        if run["status"] != "running" {
            return run;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn all_fourteen_commands_use_registered_ipc_direct_camel_case_dtos_real_http_sql_and_evidence() {
    let f = Fixture::new();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(5);
    let server = Server::new(vec![
        Response::raw(503, "application/json", "unavailable"),
        success(Protocol::OpenaiCompatible),
        success(Protocol::OpenaiCompatible),
        success(Protocol::OpenaiCompatible),
        stalled,
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], false);
    let service = f.app.iq_test.clone();
    let database = f.database.clone();
    let desktop = Arc::new(Desktop::default());
    let app = register(mock_builder())
        .manage(StartupState::Ready(f.app))
        .manage(DirectoryDesktop(desktop.clone()))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let snapshot = invoke(&window, "get_iq_test_snapshot", Value::Null).unwrap();
    keys(
        &snapshot,
        &[
            "revision",
            "questions",
            "questionSummary",
            "candidates",
            "participants",
            "activeRun",
            "runs",
        ],
    );
    assert_eq!(
        snapshot["candidates"][0]["modelKey"],
        format!("{}/{}", site.id, site.models[0].id)
    );
    assert!(snapshot["activeRun"].is_null());
    assert_eq!(snapshot["questionSummary"], json!({"total":0,"enabled":0}));
    let q = invoke(
        &window,
        "save_iq_question",
        json!({"input":{"content":"  question  ","referenceAnswer":"  answer ","enabled":true}}),
    )
    .unwrap();
    keys(
        &q,
        &[
            "id",
            "content",
            "referenceAnswer",
            "enabled",
            "createdAtMs",
            "updatedAtMs",
        ],
    );
    assert_eq!(q["content"], "question");
    let q = invoke(&window,"save_iq_question",json!({"id":q["id"],"input":{"content":"updated question","referenceAnswer":null,"enabled":true}})).unwrap();
    assert_eq!(
        invoke(
            &window,
            "set_iq_question_enabled",
            json!({"id":q["id"],"enabled":false})
        )
        .unwrap(),
        Value::Null
    );
    invoke(
        &window,
        "set_iq_question_enabled",
        json!({"id":q["id"],"enabled":true}),
    )
    .unwrap();
    let disposable = invoke(
        &window,
        "save_iq_question",
        json!({"input":{"content":"delete me","referenceAnswer":null,"enabled":false}}),
    )
    .unwrap();
    assert_eq!(
        invoke(
            &window,
            "delete_iq_question",
            json!({"id":disposable["id"]})
        )
        .unwrap(),
        Value::Null
    );
    let p = invoke(&window,"create_iq_participant_snapshot",json!({"input":{"modelKey":snapshot["candidates"][0]["modelKey"],"preset":"high label","parameters":{"reasoning_effort":"medium"}}})).unwrap();
    keys(
        &p,
        &[
            "id",
            "modelKey",
            "modelName",
            "siteId",
            "siteName",
            "protocol",
            "health",
            "endpoint",
            "snapshotId",
            "createdAtMs",
            "preset",
            "parameters",
            "precheck",
        ],
    );
    assert_eq!(p["precheck"]["status"], "checking");
    assert_eq!(p["parameters"]["reasoning_effort"], "medium");
    let failed = invoke(
        &window,
        "run_iq_precheck",
        json!({"snapshotId":p["snapshotId"]}),
    )
    .unwrap();
    keys(&failed, &["snapshotId", "precheck"]);
    assert_eq!(failed["precheck"]["status"], "unavailable");
    let refreshed = invoke(
        &window,
        "refresh_iq_participant_snapshot",
        json!({"id":p["id"]}),
    )
    .unwrap();
    assert_eq!(refreshed["id"], p["id"]);
    assert_ne!(refreshed["snapshotId"], p["snapshotId"]);
    assert_eq!(
        invoke(
            &window,
            "run_iq_precheck",
            json!({"snapshotId":refreshed["snapshotId"]})
        )
        .unwrap()["precheck"]["status"],
        "available"
    );
    let input = json!({"questionIds":[q["id"]],"retryMax":0});
    let run = invoke(&window, "start_iq_run", json!({"input":input})).unwrap();
    keys(
        &run,
        &[
            "id",
            "status",
            "questions",
            "participants",
            "entries",
            "startedAtMs",
            "finishedAtMs",
            "summary",
        ],
    );
    let run = terminal(&window, &run["id"]);
    assert_eq!(run["status"], "completed");
    assert_eq!(run["summary"]["callResult"]["value"], "all-success");
    keys(
        &run["entries"][0],
        &[
            "id",
            "runId",
            "questionIndex",
            "participantIndex",
            "status",
            "attempt",
            "startedAtMs",
            "finishedAtMs",
            "firstTokenMs",
            "totalTimeMs",
            "inputTokens",
            "outputTokens",
            "errorCode",
            "errorMessage",
            "hasRawStream",
        ],
    );
    assert_eq!(
        invoke(
            &window,
            "open_iq_raw_stream",
            json!({"entryId":run["entries"][0]["id"]})
        )
        .unwrap(),
        Value::Null
    );
    assert_eq!(desktop.0.lock().unwrap().len(), 1);
    let opened = desktop.0.lock().unwrap()[0].clone();
    assert!(opened.is_dir());
    assert!(opened.join("response.1.jsonl").is_file());
    assert!(opened.join("meta.1.json").is_file());
    let options = invoke(&window, "get_iq_history_filter_options", json!({})).unwrap();
    assert_eq!(options[0]["value"], site.id);
    assert_eq!(options[0]["children"][0]["label"], "model");
    assert_eq!(
        invoke(&window, "get_iq_test_snapshot", json!({})).unwrap()["participants"],
        json!([])
    );
    let next = invoke(&window, "create_iq_participant_snapshot", json!({"input":{"modelKey":format!("{}/{}",site.id,site.models[0].id),"preset":"high","parameters":{}}})).unwrap();
    invoke(
        &window,
        "run_iq_precheck",
        json!({"snapshotId":next["snapshotId"]}),
    )
    .unwrap();
    let replay = invoke(
        &window,
        "get_iq_entry_output",
        json!({"entryId":run["entries"][0]["id"]}),
    )
    .unwrap();
    assert_eq!(replay["attempt"], 1);
    assert!(!replay["parts"].as_array().unwrap().is_empty());
    let active = invoke(&window, "start_iq_run", json!({"input":input})).unwrap();
    tauri::async_runtime::block_on(server.wait_requests(5));
    assert_eq!(
        invoke(&window, "abort_iq_run", Value::Null).unwrap(),
        Value::Null
    );
    assert_eq!(terminal(&window, &active["id"])["status"], "aborted");
    assert_eq!(
        invoke(
            &window,
            "delete_iq_runs",
            json!({"ids":[run["id"],active["id"]]})
        )
        .unwrap(),
        Value::Null
    );
    assert_eq!(
        invoke(&window, "remove_iq_participant", json!({"id":p["id"]})).unwrap(),
        Value::Null
    );
    assert_eq!(
        invoke(&window, "get_iq_test_snapshot", json!({})).unwrap()["runs"],
        json!([])
    );
    let groups: Vec<(String,String)> = database.with_connection(|c| c.prepare("SELECT action,action_group FROM operation_logs WHERE action LIKE 'iq_%' ORDER BY id")?.query_map([],|r| Ok((r.get(0)?,r.get(1)?)))?.collect()).unwrap();
    for (action, group) in groups {
        let expected = match action.as_str() {
            "iq_run_started" | "iq_question_created" => "create",
            "iq_question_updated" => "update",
            "iq_run_aborted" => "status_change",
            _ => "delete",
        };
        assert_eq!(group, expected, "{action}");
    }
    tauri::async_runtime::block_on(service.stop());
}

#[test]
fn ipc_rejects_missing_wrong_reserved_and_unknown_fields_and_returns_structured_domain_errors() {
    let f = Fixture::new();
    let service = f.app.iq_test.clone();
    let app = register(mock_builder())
        .manage(StartupState::Ready(f.app))
        .manage(DirectoryDesktop(Arc::new(Desktop::default())))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for (command, body) in [
        (
            "save_iq_question",
            json!({"input":{"content":"missing referenceAnswer","enabled":true}}),
        ),
        (
            "save_iq_question",
            json!({"input":{"content":"x","referenceAnswer":null,"enabled":true,"secret":"private-input"}}),
        ),
        (
            "create_iq_participant_snapshot",
            json!({"input":{"modelKey":"missing","preset":"x","parameters":[]}}),
        ),
        (
            "start_iq_run",
            json!({"input":{"questionIds":["x"],"timeoutSeconds":120.5,"timeoutRetryMax":5}}),
        ),
        ("run_iq_precheck", json!({"snapshot_id":"x"})),
        ("get_iq_run_details", json!({"runId":"x"})),
        (
            "open_iq_raw_stream",
            json!({"entryId":"x","path":"private-input"}),
        ),
        ("delete_iq_runs", json!({"ids":[false]})),
        ("get_iq_test_snapshot", json!({"unknown":"private-input"})),
    ] {
        let error = invoke(&window, command, body).unwrap_err();
        keys(
            &error,
            &[
                "code",
                "message",
                "category",
                "retryable",
                "field",
                "operationId",
            ],
        );
        assert_eq!(error["code"], "invalid_input");
        assert_eq!(error["category"], "validation");
        assert_eq!(error["retryable"], false);
        assert!(error["operationId"].is_null());
        assert!(!error.to_string().contains("private-input"));
    }
    let error = invoke(&window, "get_iq_run_details", json!({"id":"missing"})).unwrap_err();
    assert_eq!(error["code"], "iq_not_found");
    assert_eq!(error["category"], "not_found");
    for field in ["timeoutSeconds", "timeoutRetryMax"] {
        let error = invoke(
            &window,
            "start_iq_run",
            json!({"input":{"questionIds":["q"],"retryMax":0,field:120}}),
        )
        .unwrap_err();
        assert_eq!(error["code"], "invalid_input");
    }
    for (input, code) in [
        (json!({"questionIds":["q"]}), "invalid_input"),
        (json!({"questionIds":["q"],"retryMax":-1}), "invalid_input"),
        (json!({"questionIds":["q"],"retryMax":1.5}), "invalid_input"),
        (json!({"questionIds":["q"],"retryMax":21}), "iq_validation"),
    ] {
        let error = invoke(&window, "start_iq_run", json!({"input":input})).unwrap_err();
        assert_eq!(error["code"], code);
        assert_eq!(error["category"], "validation");
    }
    tauri::async_runtime::block_on(service.stop());
}

#[test]
fn all_fourteen_iq_commands_report_unavailable_when_startup_is_blocked() {
    let state = crate::startup::initialize(Err(
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
        ("get_iq_test_snapshot", Value::Null),
        (
            "save_iq_question",
            json!({"input":{"content":"q","referenceAnswer":null,"enabled":true}}),
        ),
        ("delete_iq_question", json!({"id":"q"})),
        ("set_iq_question_enabled", json!({"id":"q","enabled":true})),
        (
            "create_iq_participant_snapshot",
            json!({"input":{"modelKey":"s/m","preset":"x","parameters":{}}}),
        ),
        ("refresh_iq_participant_snapshot", json!({"id":"p"})),
        ("run_iq_precheck", json!({"snapshotId":"s"})),
        ("remove_iq_participant", json!({"id":"p"})),
        (
            "start_iq_run",
            json!({"input":{"questionIds":["q"],"retryMax":0}}),
        ),
        ("abort_iq_run", Value::Null),
        ("get_iq_history_filter_options", Value::Null),
        ("get_iq_run_details", json!({"id":"r"})),
        ("delete_iq_runs", json!({"ids":["r"]})),
        ("open_iq_raw_stream", json!({"entryId":"e"})),
    ] {
        let error = invoke(&window, command, body).unwrap_err();
        assert_eq!(error["code"], "iq_unavailable", "{command}");
        assert_eq!(error["category"], "unavailable");
        assert_eq!(error["retryable"], true);
        assert!(error["field"].is_null() && error["operationId"].is_null());
    }
}
