use super::*;
use crate::{
    commands::register,
    desktop::diagnostics::DiagnosticsPlatform,
    shared::persistence::test_support::environment,
    startup::{self, StartupState},
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

#[derive(Default)]
struct FakeDesktop {
    destination: Mutex<Option<PathBuf>>,
    opened: Mutex<Vec<PathBuf>>,
    fail: Mutex<bool>,
}
impl DiagnosticsPlatform for FakeDesktop {
    fn open_directory(&self, path: &Path) -> Result<(), DiagnosticsPlatformError> {
        if *self.fail.lock().unwrap() {
            return Err(DiagnosticsPlatformError::OpenDirectory);
        }
        self.opened.lock().unwrap().push(path.to_owned());
        Ok(())
    }
    fn save_destination(&self) -> Result<Option<PathBuf>, DiagnosticsPlatformError> {
        if *self.fail.lock().unwrap() {
            return Err(DiagnosticsPlatformError::SaveDialog);
        }
        Ok(self.destination.lock().unwrap().clone())
    }
}

fn invoke(window: &WebviewWindow<MockRuntime>, command: &str) -> Result<Value, Value> {
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
            body: InvokeBody::Json(json!({})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .map(|body| body.deserialize().unwrap())
}

#[test]
fn four_registered_commands_work_even_when_database_startup_fails() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let database = env.paths().data_root().join("relaypulse.sqlite3");
    fs::write(&database, b"broken database").unwrap();
    let (state, diagnostics) = startup::initialize(Ok(env));
    assert!(matches!(state, StartupState::Blocked(_)));
    let resource = diagnostics.0.clone().unwrap();
    let platform = Arc::new(FakeDesktop::default());
    let app = register(mock_builder())
        .manage(state)
        .manage(diagnostics)
        .manage(DiagnosticsDesktop(platform.clone()))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let statistics = invoke(&window, "get_log_statistics").unwrap();
    assert_eq!(statistics["fileCount"], 1);
    assert_eq!(statistics.as_object().unwrap().len(), 2);
    assert!(statistics["totalBytes"].as_u64().unwrap() > 0);
    let destination = temp.path().join("diagnostics.json");
    assert_eq!(
        invoke(&window, "export_diagnostics").unwrap_err(),
        json!({
            "code": "diagnostics_export_cancelled",
            "category": "cancelled",
            "message": "已取消导出诊断包。",
            "field": null,
            "retryable": false,
            "operationId": null
        })
    );
    assert!(!destination.exists());
    assert_eq!(invoke(&window, "get_log_statistics").unwrap(), statistics);
    *platform.destination.lock().unwrap() = Some(destination.clone());
    assert_eq!(invoke(&window, "export_diagnostics").unwrap(), true);
    let package: Value = serde_json::from_slice(&fs::read(destination).unwrap()).unwrap();
    assert_eq!(package["startup"]["error"], "persistence");
    assert_eq!(package["logStatistics"], statistics);
    invoke(&window, "open_log_directory").unwrap();
    assert_eq!(
        *platform.opened.lock().unwrap(),
        vec![temp
            .path()
            .join("com.relaypulse.app.dev/logs")
            .canonicalize()
            .unwrap()]
    );
    *platform.destination.lock().unwrap() = Some(temp.path().join("missing/export.json"));
    assert_eq!(
        invoke(&window, "export_diagnostics").unwrap_err()["code"],
        "diagnostics_failed"
    );
    *platform.fail.lock().unwrap() = true;
    assert_eq!(
        invoke(&window, "open_log_directory").unwrap_err()["code"],
        "diagnostics_failed"
    );
    assert_eq!(
        invoke(&window, "export_diagnostics").unwrap_err()["code"],
        "diagnostics_failed"
    );
    assert_eq!(
        invoke(&window, "clear_diagnostic_logs").unwrap(),
        json!({"totalBytes":0,"fileCount":0})
    );
    resource
        .record(
            "configuration_load",
            crate::shared::diagnostics::Outcome::Succeeded,
        )
        .unwrap();
    assert_eq!(
        invoke(&window, "get_log_statistics").unwrap()["fileCount"],
        1
    );
    assert_eq!(fs::read(database).unwrap(), b"broken database");
}

#[test]
fn environment_failure_does_not_fake_zero_statistics() {
    let (state, diagnostics) = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ));
    let app = register(mock_builder())
        .manage(state)
        .manage(diagnostics)
        .manage(DiagnosticsDesktop(Arc::new(FakeDesktop::default())))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for command in [
        "get_log_statistics",
        "clear_diagnostic_logs",
        "open_log_directory",
        "export_diagnostics",
    ] {
        assert_eq!(
            invoke(&window, command).unwrap_err()["code"],
            "diagnostics_unavailable"
        );
    }
}
