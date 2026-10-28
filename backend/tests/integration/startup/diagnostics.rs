//! 通过真实组装验证业务事件投影、敏感字段隔离和诊断降级。
use crate::{
    modules::notifications::{
        tests::FakePlatform, NotificationPermission, TemplateMode, TemplateUpdate, TestNotification,
    },
    shared::persistence::test_support::environment,
    startup::{initialize, StartupState},
};
use serde_json::Value;
use std::fs;

#[test]
fn business_operations_log_safe_events_without_settings_templates_or_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    let (state, diagnostics) = initialize(Ok(env));
    let StartupState::Ready(app) = state else {
        panic!("expected ready");
    };
    let diagnostics = diagnostics.0.unwrap();
    let mut settings = app.configuration.load().unwrap();
    settings.history_retention_days = 0;
    assert!(app.configuration.save(settings).is_err());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut notifications = app.notifications.load(&platform).unwrap();
    notifications.notifications_enabled = true;
    app.notifications.save(notifications, &platform).unwrap();
    app.notifications
        .save_template(
            "probe.model_failed",
            TemplateUpdate {
                template_type: TemplateMode::Custom,
                marker_id: "none".into(),
                title_template: "credential=SECRET token=TOKEN".into(),
                body_template: "requestBody=PRIVATE responseBody=RESPONSE".into(),
            },
        )
        .unwrap();
    app.notifications
        .send_test(
            TestNotification {
                title: "credential=SECRET token=TOKEN".into(),
                body: "requestBody=PRIVATE responseBody=RESPONSE".into(),
                marker_id: None,
            },
            &platform,
        )
        .unwrap();
    *platform.fail.lock().unwrap() = Some("send");
    assert!(app
        .notifications
        .send_test(
            TestNotification {
                title: "credential=SECRET token=TOKEN".into(),
                body: "requestBody=PRIVATE responseBody=RESPONSE".into(),
                marker_id: None,
            },
            &platform
        )
        .is_err());
    let logs = fs::read_to_string(root.join("logs/diagnostic-events.jsonl")).unwrap();
    let package = String::from_utf8(diagnostics.package().unwrap()).unwrap();
    for value in [logs, package.clone()] {
        for secret in [
            "SECRET",
            "TOKEN",
            "PRIVATE",
            "RESPONSE",
            "requestBody",
            "responseBody",
            "titleTemplate",
            "historyRetentionDays",
        ] {
            assert!(!value.contains(secret));
        }
    }
    let package: Value = serde_json::from_str(&package).unwrap();
    let events = package["events"].as_array().unwrap();
    for operation in [
        "startup",
        "configuration_load",
        "configuration_save",
        "notification_initialize",
        "notification_load",
        "notification_save",
        "notification_template_save",
        "notification_test",
    ] {
        assert!(events.iter().any(|event| event["operation"] == operation));
    }
    assert!(events
        .iter()
        .any(|event| event["outcome"]["error"] == "validation"));
    assert!(events
        .iter()
        .any(|event| event["outcome"]["error"] == "platform"));
    // 清理诊断日志不能影响已经保存的设置与通知内容。
    diagnostics.clear().unwrap();
    assert!(
        app.notifications
            .load(&platform)
            .unwrap()
            .notifications_enabled
    );
    assert_eq!(
        app.notifications
            .template("probe.model_failed")
            .unwrap()
            .body_template,
        "requestBody=PRIVATE responseBody=RESPONSE"
    );
    assert_eq!(app.configuration.load().unwrap().history_retention_days, 30);
}

#[test]
fn unavailable_log_directory_does_not_block_business_initialization_or_saving() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let logs = env.paths().data_root().join("logs");
    fs::write(&logs, b"occupied").unwrap();
    let (state, diagnostics) = initialize(Ok(env));
    let StartupState::Ready(app) = state else {
        panic!("logging must not block settings");
    };
    let mut settings = app.configuration.load().unwrap();
    settings.history_retention_days = 90;
    assert_eq!(
        app.configuration
            .save(settings)
            .unwrap()
            .history_retention_days,
        90
    );
    assert!(diagnostics.0.unwrap().statistics().is_err());
    assert_eq!(fs::read(logs).unwrap(), b"occupied");
}

#[test]
fn raw_storage_errors_are_never_serialized_into_diagnostics() {
    use crate::{
        modules::configuration::{ConfigurationService, GeneralSettings, SettingsStore},
        shared::diagnostics::Diagnostics,
    };
    use std::sync::Arc;
    struct FailingStore;
    impl SettingsStore for FailingStore {
        fn load(&self) -> Result<GeneralSettings, Box<dyn std::error::Error + Send + Sync>> {
            Err(std::io::Error::other(
                "credential=SECRET token=TOKEN requestBody=PRIVATE responseBody=RESPONSE",
            )
            .into())
        }
        fn save(
            &self,
            _: &GeneralSettings,
        ) -> Result<GeneralSettings, Box<dyn std::error::Error + Send + Sync>> {
            self.load()
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let diagnostics = Arc::new(Diagnostics::new(&env));
    let service = ConfigurationService::new(FailingStore).with_diagnostics(diagnostics.clone());
    assert!(service.load().is_err());
    let package = String::from_utf8(diagnostics.package().unwrap()).unwrap();
    let logs = fs::read_to_string(
        diagnostics
            .log_directory()
            .unwrap()
            .join("diagnostic-events.jsonl"),
    )
    .unwrap();
    for content in [package, logs] {
        assert!(content.contains("storage"));
        for secret in [
            "SECRET",
            "TOKEN",
            "PRIVATE",
            "RESPONSE",
            "requestBody",
            "responseBody",
        ] {
            assert!(!content.contains(secret));
        }
    }
}
