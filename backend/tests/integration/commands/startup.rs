use super::*;
use crate::{
    commands::register,
    desktop::RestartPlatform,
    legacy_data,
    shared::persistence::test_support::environment,
    startup::{self, StartupState},
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewWindow, WebviewWindowBuilder,
};

#[derive(Default)]
struct FakeRestart(AtomicUsize);
impl RestartPlatform for FakeRestart {
    fn restart(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
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
fn registered_startup_commands_migrate_only_the_host_directory_and_restart_after_verification() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    legacy_data::create(&root);
    let original = std::fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let state = startup::initialize(Ok(env)).0;
    assert!(matches!(state, StartupState::Blocked(_)));
    let recovery = Arc::new(StartupRecovery::new(Some(root.clone()), &state));
    let desktop = Arc::new(FakeRestart::default());
    let app = register(mock_builder())
        .manage(state)
        .manage(recovery)
        .manage(StartupDesktop(desktop.clone()))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&window, "get_startup_status").unwrap()["status"],
        "migration_required"
    );
    assert_eq!(
        invoke(&window, "restart_after_data_migration").unwrap_err()["code"],
        "startup_migration_not_completed"
    );
    assert_eq!(desktop.0.load(Ordering::SeqCst), 0);
    let outcome = invoke(&window, "migrate_startup_data").unwrap();
    let backup = std::path::Path::new(outcome["backupDirectory"].as_str().unwrap());
    assert_eq!(
        std::fs::read(backup.join("relaypulse.sqlite3")).unwrap(),
        original
    );
    assert_eq!(
        invoke(&window, "get_startup_status").unwrap()["status"],
        "migration_completed"
    );
    assert_eq!(
        invoke(&window, "restart_after_data_migration").unwrap(),
        Value::Null
    );
    assert_eq!(desktop.0.load(Ordering::SeqCst), 1);
}

#[test]
fn a_ready_application_rejects_migration_and_unauthorized_restart() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    let state = startup::initialize(Ok(env)).0;
    let recovery = Arc::new(StartupRecovery::new(Some(root), &state));
    let desktop = Arc::new(FakeRestart::default());
    let app = register(mock_builder())
        .manage(state)
        .manage(recovery)
        .manage(StartupDesktop(desktop.clone()))
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&window, "get_startup_status").unwrap()["status"],
        "ready"
    );
    assert_eq!(
        invoke(&window, "migrate_startup_data").unwrap_err()["category"],
        "startup"
    );
    assert!(invoke(&window, "restart_after_data_migration").is_err());
    assert_eq!(desktop.0.load(Ordering::SeqCst), 0);
}
