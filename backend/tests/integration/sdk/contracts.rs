//! External consumer contract: new business definitions, no SDK or shared SQL changes.
use relaypulse_diagnostics::{Diagnostics, Failure, Outcome};
use relaypulse_notifications::*;
use relaypulse_operations::*;
use relaypulse_runtime::{environment::EnvironmentContext, file_output::OutputStore};
use relaypulse_storage::Database;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct Platform(Mutex<Vec<(String, String, Option<NotificationAction>)>>);
impl NotificationPlatform for Platform {
    fn permission(&self) -> Result<NotificationPermission, NotificationError> {
        Ok(NotificationPermission::Granted)
    }
    fn request_permission(&self) -> Result<NotificationPermission, NotificationError> {
        self.permission()
    }
    fn send(
        &self,
        title: &str,
        body: &str,
        action: Option<NotificationAction>,
    ) -> Result<(), NotificationError> {
        self.0
            .lock()
            .unwrap()
            .push((title.into(), body.into(), action));
        Ok(())
    }
    fn open_settings(&self) -> Result<(), NotificationError> {
        Ok(())
    }
}
#[tokio::test]
async fn independently_defined_business_uses_frozen_public_capabilities() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let db = Arc::new(
        Database::open_directory(
            &root,
            &[
                relaypulse_notifications::SCHEMA,
                relaypulse_operations::SCHEMA,
            ],
        )
        .unwrap(),
    );
    let event = RuleDefinition {
        id: "fixture.analysis_ready",
        title: "分析完成",
        sub_title: "测试扩展",
        marker: "none",
        title_template: "{{document}} 已完成",
        body_template: "{{count}} 项",
    };
    let variables = vec![
        NotificationVariable {
            key: "document",
            label: "文档",
            description: "文档名",
            example: "样例",
            group: "扩展",
        },
        NotificationVariable {
            key: "count",
            label: "数量",
            description: "处理数量",
            example: "3",
            group: "扩展",
        },
    ];
    let catalog = EventCatalog::new(vec![event], variables).unwrap();
    let notifications =
        NotificationService::new(SqliteNotificationStore::new(db.clone()), catalog.clone());
    let platform = Platform(Mutex::new(vec![]));
    let mut settings = notifications.load(&platform).unwrap();
    settings.notifications_enabled = true;
    notifications.save(settings, &platform).unwrap();
    notifications
        .save_template(
            "fixture.analysis_ready",
            TemplateUpdate {
                template_type: TemplateMode::Custom,
                marker_id: "none".into(),
                title_template: "完成 {{document}}".into(),
                body_template: "共 {{count}} 项".into(),
            },
        )
        .unwrap();
    let action = NotificationAction {
        target: "fixture_document".into(),
        parameters: BTreeMap::from([("id".into(), "document-7".into())]),
    };
    notifications
        .send_rule(
            "fixture.analysis_ready",
            &NotificationContext {
                values: BTreeMap::from([("document", "报告".into()), ("count", "3".into())]),
                action: action.clone(),
            },
            &platform,
        )
        .unwrap();
    assert_eq!(
        platform.0.lock().unwrap()[0],
        ("完成 报告".into(), "共 3 项".into(), Some(action))
    );
    let logs = Arc::new(OperationLogs::new(db.clone()));
    let definition = ActionDefinition::new("fixture.analysis_completed", "完成分析", "analysis");
    let mut event = OperationEvent::new(
        definition,
        OperationSource::User,
        OperationResult::Success,
        "报告",
        "已处理 3 项",
        OperationLogContent::Presentation {
            format: "fixture.analysis_summary".into(),
            fields: vec![SnapshotField {
                label: "api_key".into(),
                value: "never-store-this".into(),
                sensitive: Some(true),
            }],
        },
    );
    event.detail_behavior = DetailBehavior::Navigate {
        target: "fixture_document".into(),
        parameters: BTreeMap::from([("id".into(), "document-7".into())]),
    };
    logs.record(event).unwrap();
    let query = OperationLogQuery {
        start_date: None,
        end_date: None,
        source: vec![],
        result: vec![],
        actions: vec!["analysis".into()],
        page: 1,
        page_size: 20,
    };
    let page = logs.query(query).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].action, "fixture.analysis_completed");
    assert_eq!(page.items[0].action_label, "完成分析");
    let detail = logs.detail(page.items[0].id.clone()).await.unwrap();
    assert!(!serde_json::to_string(&detail)
        .unwrap()
        .contains("never-store-this"));
    // A later consumer with a different catalog still deserializes historical IDs and snapshots.
    let stored = serde_json::to_value(detail).unwrap();
    let decoded: OperationLogDetail = serde_json::from_value(stored).unwrap();
    assert_eq!(decoded.item.action, "fixture.analysis_completed");
    let output = OutputStore::new(root.join("analysis-evidence"));
    let session = output.start("document-7".into()).await.unwrap();
    session
        .append("result.txt", b"verified output")
        .await
        .unwrap();
    session.publish().await.unwrap();
    drop(session);
    output.close();
    output.drained().await;
    assert_eq!(
        std::fs::read(root.join("analysis-evidence/document-7/result.txt")).unwrap(),
        b"verified output"
    );
    let env = EnvironmentContext::resolve(
        "org.external.analysis",
        &root.join("org.external.analysis"),
        &[relaypulse_runtime::EnvironmentDefinition {
            identifier: "org.external.analysis",
            environment: relaypulse_runtime::AppEnvironment::Development,
            data_root: root.join("org.external.analysis"),
        }],
        relaypulse_runtime::ApplicationInfo::new("ExternalAnalysis", "77.8.9").unwrap(),
    )
    .unwrap();
    let diagnostics = Diagnostics::new(&env);
    diagnostics
        .record(
            "fixture.analysis_completed",
            Outcome::Failed(Failure::code("fixture_upstream_rejected")),
        )
        .unwrap();
    let package = String::from_utf8(diagnostics.package().unwrap()).unwrap();
    assert!(package.contains("fixture_upstream_rejected"));
    assert!(package.contains("ExternalAnalysis"));
    assert!(package.contains("77.8.9"));
    assert!(!package.contains("probeSummary"));
}

#[tokio::test]
async fn third_consumer_controls_parameters_capture_and_cancellation() {
    use relaypulse_model_client::*;
    use std::{future::Future, pin::Pin, time::Duration};
    #[derive(Default)]
    struct Capture {
        body: Option<serde_json::Value>,
        finished: bool,
    }
    impl CaptureSink for Capture {
        fn record<'a>(
            &'a mut self,
            event: CaptureEvent<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<(), CaptureError>> + Send + 'a>> {
            match event {
                CaptureEvent::Started { body, .. } => self.body = Some(body.clone()),
                CaptureEvent::Finished { .. } => self.finished = true,
                _ => {}
            }
            Box::pin(async { Ok(()) })
        }
    }
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let parameters =
        serde_json::json!({"vendor_options":{"budget":512},"temperature":"caller-owned-type"})
            .as_object()
            .unwrap()
            .clone();
    // Validation checks structural safety; provider and business policies belong to the caller.
    validate_generation_parameters(&parameters).unwrap();
    let request = GenerationRequest {
        target: RequestTarget {
            protocol: Protocol::OpenaiCompatible,
            base_url: "http://127.0.0.1:1".into(),
            headers: vec![],
            secret: String::from("fixture-credential").into(),
        },
        model: "fixture-model".into(),
        endpoint: "/v1/chat/completions".into(),
        prompt: "fixture prompt".into(),
        parameters,
    };
    let mut capture = Capture::default();
    let result = ModelClient::default()
        .generate_once(
            &request,
            GenerationOptions {
                output_timeout: Some(Duration::from_secs(1)),
            },
            &cancellation,
            &mut capture,
        )
        .await;
    let result = result.unwrap();
    assert_eq!(result.failure, Some(GenerationFailure::Cancelled));
    assert!(capture.body.is_none());
    assert!(capture.finished);
}
