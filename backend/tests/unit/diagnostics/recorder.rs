use super::*;
use crate::shared::persistence::test_support::environment;
use serde_json::{json, Value};
use std::{cell::Cell, fs};

// 特意不实现 Serialize / Debug / Display：公共记录流程不能依赖业务数据的序列化。
struct ModuleError(Box<str>);
impl DiagnosticFailure for ModuleError {
    fn diagnostic_failure(&self) -> Failure {
        Failure::Validation
    }
}

fn error() -> ModuleError {
    ModuleError("token=SECRET requestBody=PRIVATE responseBody=RESPONSE".into())
}

#[test]
fn module_operations_need_no_registry_and_results_are_not_serialized() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = Arc::new(Diagnostics::new(&environment(
        temp.path(),
        "com.relaypulse.app.dev",
    )));
    let recorder = Recorder::new(diagnostics.clone());
    let calls = Cell::new(0);
    let success: Result<_, ModuleError> = recorder.observe("example_module_load", || {
        calls.set(calls.get() + 1);
        Ok(Box::<str>::from("credential=SECRET"))
    });
    assert_eq!(success.ok().unwrap().as_ref(), "credential=SECRET");
    let failure: Result<(), _> = recorder.observe("example_module_save", || {
        calls.set(calls.get() + 1);
        Err(error())
    });
    assert_eq!(failure.err().unwrap().0.as_ref(), error().0.as_ref());
    assert_eq!(calls.get(), 2);

    let package: Value = serde_json::from_slice(&diagnostics.package().unwrap()).unwrap();
    let events = package["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["operation"], "example_module_load");
    assert_eq!(events[0]["outcome"], json!({"status":"succeeded"}));
    assert_eq!(events[1]["operation"], "example_module_save");
    assert_eq!(
        events[1]["outcome"],
        json!({"status":"failed", "error":"validation"})
    );
    let logs = fs::read_to_string(
        diagnostics
            .log_directory()
            .unwrap()
            .join("diagnostic-events.jsonl"),
    )
    .unwrap();
    for content in [package.to_string(), logs] {
        for secret in [
            "credential",
            "SECRET",
            "token",
            "requestBody",
            "PRIVATE",
            "responseBody",
            "RESPONSE",
        ] {
            assert!(!content.contains(secret));
        }
    }
}

#[test]
fn without_a_file_resource_the_action_still_runs_once_and_returns_its_error() {
    let recorder = Recorder::default();
    let calls = Cell::new(0);
    let result: Result<(), _> = recorder.observe("example_module_save", || {
        calls.set(calls.get() + 1);
        Err(error())
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(result.err().unwrap().0.as_ref(), error().0.as_ref());
}

#[test]
fn log_failure_does_not_replace_success_or_original_business_error() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let log_path = env.paths().data_root().join("logs");
    fs::write(&log_path, "occupied").unwrap();
    let recorder = Recorder::new(Arc::new(Diagnostics::new(&env)));
    let success: Result<_, ModuleError> = recorder.observe("example_module_load", || Ok(42));
    assert_eq!(success.ok(), Some(42));
    let failure: Result<(), _> = recorder.observe("example_module_save", || Err(error()));
    assert_eq!(failure.err().unwrap().0.as_ref(), error().0.as_ref());
    assert_eq!(fs::read_to_string(&log_path).unwrap(), "occupied");
    // 一次写入失败不会像任务输出会话那样永久标记失败；下次可重新打开日志。
    fs::remove_file(&log_path).unwrap();
    let retried: Result<_, ModuleError> = recorder.observe("example_module_retry", || Ok(43));
    assert_eq!(retried.ok(), Some(43));
    let events = fs::read_to_string(log_path.join("diagnostic-events.jsonl")).unwrap();
    let events: Vec<Value> = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["operation"], "example_module_retry");
}
