use super::*;
use crate::{
    application::sites::Operation,
    modules::sites::{CredentialAction, SiteDraft},
    shared::model_client::RequestHeader,
};

#[tokio::test]
async fn missing_anthropic_max_tokens_is_sent_unchanged_rejected_silently_and_blocks_start() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(
        400,
        "application/json",
        json!({"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: Field required"}}).to_string(),
    )]);
    let site = f.site(&server.url, Protocol::Anthropic, &["actual-model"], false);
    let parameters =
        json!({"thinking":{"type":"enabled","budget_tokens":32000},"vendor_flag":true});
    let p = f.participant(&site, 0, parameters.clone()).await;
    let question = f.question("这道正式题不能进入预检").await;
    let audit_before = f.count("operation_logs");
    let panel_before = f.app.panel.changes().version();
    let files_before = support::files(f.app.environment.paths().data_root());
    let result = f.app.iq_test.precheck(p.snapshot_id.clone()).await.unwrap();
    assert_eq!(result.snapshot_id, p.snapshot_id);
    assert_eq!(result.precheck.status, PrecheckStatus::Unavailable);
    assert!(result
        .precheck
        .message
        .contains("max_tokens: Field required"));
    assert!(result.precheck.message.contains("400"));
    assert_eq!(f.count("operation_logs"), audit_before);
    assert_eq!(f.app.panel.changes().version(), panel_before);
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    assert_eq!(
        support::files(f.app.environment.paths().data_root()),
        files_before
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].head.contains(&format!("x-api-key: {SECRET}")));
    assert!(!requests[0].head.contains("anthropic-version:"));
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "actual-model");
    assert_eq!(body["stream"], true);
    assert_eq!(
        body["messages"],
        json!([{"role":"user","content":"只回复 OK"}])
    );
    for (key, value) in parameters.as_object().unwrap() {
        assert_eq!(body.get(key), Some(value));
    }
    for key in ["max_tokens", "max_output_tokens", "max_completion_tokens"] {
        assert!(body.get(key).is_none(), "{key}");
    }
    assert!(matches!(
        f.app
            .iq_test
            .start_run(IqStartInput {
                question_ids: vec![question.id],
                retry_max: 0,
            })
            .await,
        Err(IqError::Validation {
            field: "participants",
            ..
        })
    ));
    assert_eq!(f.count("iq_runs"), 0);
    assert_eq!(server.requests().len(), 1);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn anthropic_precheck_generation_and_history_use_only_enabled_provider_headers() {
    use crate::shared::model_client::{Parameter, ParameterType};
    for enabled in [None, Some(false), Some(true)] {
        let f = Fixture::new();
        let server = Server::new(vec![success(Protocol::Anthropic); 2]);
        let site = f.site(&server.url, Protocol::Anthropic, &["remote"], false);
        let mut headers = vec![];
        if let Some(enabled) = enabled {
            let mut header =
                Parameter::new("anthropic-version", "user-version", ParameterType::String);
            header.enabled = enabled;
            headers.push(header);
        }
        let p = f
            .app
            .iq_test
            .create_participant(IqParticipantInput {
                model_key: format!("{}/{}", site.id, site.models[0].id),
                preset: "custom".into(),
                headers: headers.clone(),
                parameters: Default::default(),
            })
            .await
            .unwrap();
        assert_eq!(
            f.app
                .iq_test
                .precheck(p.snapshot_id)
                .await
                .unwrap()
                .precheck
                .status,
            PrecheckStatus::Available
        );
        let question = f.question("test visible headers").await;
        let run = f.start(&[question]).await;
        let run = f.terminal(&run.id).await;
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.participants[0].headers, headers);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert!(request.head.contains(&format!("x-api-key: {SECRET}")));
            assert_eq!(
                request.head.contains("anthropic-version: user-version"),
                enabled == Some(true)
            );
        }
        let mut metas = 0;
        for (path, bytes) in support::files(&f.root()) {
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("meta."))
            {
                let meta: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    meta["participant"]["headers"],
                    serde_json::to_value(&headers).unwrap()
                );
                metas += 1;
            }
        }
        assert_eq!(metas, 1);
        f.app.iq_test.stop().await;
    }
}

#[tokio::test]
async fn every_protocol_precheck_and_generation_preserve_the_same_user_parameter_object() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![success(protocol); 2]);
        let site = f.site(&server.url, protocol, &["actual-model"], true);
        let mut parameters = json!({
            "thinking":{"type":"enabled","budget_tokens":32000},
            "output_config":{"effort":"max"},
            "reasoning":{"effort":"high"},
            "reasoning_effort":"medium",
            "temperature":0.37,
            "vendor_feature":{"enabled":true,"options":[null,"保留自定义值",9007199254740993u64]}
        });
        let token_field = if protocol == Protocol::OpenaiResponses {
            "max_output_tokens"
        } else {
            "max_tokens"
        };
        parameters[token_field] = json!(64000);
        let question = f.question("正式执行当前题目").await;
        let p = f.available(&site, 0, parameters.clone()).await;
        assert_eq!(server.requests().len(), 1, "预检只发起验证消息");
        let run = f.start(std::slice::from_ref(&question)).await;
        let run = f.terminal(&run.id).await;
        assert_eq!(run.participants[0].snapshot_id, p.snapshot_id);
        assert_eq!(run.summary.call_result.value, CallResultValue::AllSuccess);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        for (request, prompt) in requests
            .iter()
            .zip(["只回复 OK", question.content.as_str()])
        {
            let mut expected = parameters.clone();
            expected["model"] = json!("actual-model");
            expected["stream"] = json!(true);
            let message_field = if protocol == Protocol::OpenaiResponses {
                "input"
            } else {
                "messages"
            };
            expected[message_field] = json!([{"role":"user","content":prompt}]);
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body, expected, "{protocol:?}");
        }
        assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
        f.app.iq_test.stop().await;
    }
}

#[tokio::test]
async fn rejected_parameter_reason_redacts_current_old_and_authorization_credentials_after_refresh()
{
    const OLD_AUTH: &str = "old-proxy-auth-token";
    const NEW_AUTH: &str = "new-proxy-auth-token";
    const NEW_KEY: &str = "rotated-key-with-quote-\"and-backslash-\\";
    const OLD_HEADER: &str = "old-custom-header-token";
    const NEW_HEADER: &str = "new-custom-header-token";
    let rejection = |message| {
        Response::raw(
            400,
            "application/json",
            json!({
                "error":{"message":message,"type":"invalid_request_error"},
                "request_headers":{"private_marker":"never-return-the-whole-response"}
            })
            .to_string(),
        )
    };
    let mut rotated_error = rejection(format!(
        "unsupported parameter vendor_feature; old={SECRET}; current={NEW_KEY}; old-auth={OLD_AUTH}; old-header={OLD_HEADER}"
    ));
    let body = rotated_error.parts.remove(0).1;
    rotated_error.parts = body
        .chunks(37)
        .map(|chunk| (Duration::from_millis(1), chunk.to_vec()))
        .collect();
    let server = Server::new(vec![
        rejection(format!(
            "unsupported parameter vendor_feature; key={SECRET}; auth={OLD_AUTH}"
        )),
        rotated_error,
    ]);
    let f = Fixture::new();
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    let mut connection = site.connection.clone();
    connection.headers = vec![
        RequestHeader {
            key: "x-vendor-authorization".into(),
            value: format!("Bearer {OLD_AUTH}"),
        },
        RequestHeader {
            key: "x-client-token".into(),
            value: OLD_HEADER.into(),
        },
    ];
    f.sites.save_connection(&site.id, &connection).unwrap();
    let parameters = json!({"vendor_feature":true,"max_tokens":48000});
    let p = f
        .app
        .iq_test
        .create_participant(IqParticipantInput {
            model_key: format!("{}/{}", site.id, site.models[0].id),
            preset: "custom".into(),
            headers: connection
                .headers
                .iter()
                .map(|header| {
                    crate::shared::model_client::Parameter::new(
                        &header.key,
                        &header.value,
                        crate::shared::model_client::ParameterType::String,
                    )
                })
                .collect(),
            parameters: parameters.as_object().unwrap().clone(),
        })
        .await
        .unwrap();
    let old = f.app.iq_test.precheck(p.snapshot_id.clone()).await.unwrap();
    assert_eq!(old.precheck.status, PrecheckStatus::Unavailable);
    assert!(old
        .precheck
        .message
        .contains("unsupported parameter vendor_feature"));
    assert!(!old.precheck.message.contains(SECRET));
    assert!(!old.precheck.message.contains(OLD_AUTH));
    f.app
        .sites
        .execute(Operation::Save(
            site.id.clone(),
            SiteDraft {
                name: site.name.clone(),
                protocol: site.protocol,
                base_url: server.url.clone(),
                credential_action: CredentialAction::Replace,
                secret: NEW_KEY.into(),
            },
        ))
        .await
        .unwrap();
    connection.headers[0].value = format!("Bearer {NEW_AUTH}");
    connection.headers[1].value = NEW_HEADER.into();
    f.app
        .sites
        .execute(Operation::Connection(site.id.clone(), connection))
        .await
        .unwrap();
    let next = f
        .app
        .iq_test
        .refresh_participant(p.id.clone())
        .await
        .unwrap();
    assert_eq!(next.id, p.id);
    assert_ne!(next.snapshot_id, p.snapshot_id);
    assert!(matches!(
        f.app.iq_test.precheck(p.snapshot_id).await,
        Err(IqError::NotFound)
    ));
    let audit_before = f.count("operation_logs");
    let panel_before = f.app.panel.changes().version();
    let logs_before = support::files(&f.app.environment.paths().data_root().join("logs"));
    let result = f
        .app
        .iq_test
        .precheck(next.snapshot_id.clone())
        .await
        .unwrap();
    assert_eq!(result.precheck.status, PrecheckStatus::Unavailable);
    assert!(result
        .precheck
        .message
        .contains("unsupported parameter vendor_feature"));
    assert!(result.precheck.message.contains("[REDACTED]"));
    for secret in [
        SECRET,
        NEW_KEY,
        OLD_AUTH,
        OLD_HEADER,
        "never-return-the-whole-response",
    ] {
        assert!(!result.precheck.message.contains(secret), "{secret}");
    }
    let snapshot = f.app.iq_test.snapshot().await.unwrap();
    assert_eq!(
        snapshot.participants[0].precheck.message,
        result.precheck.message
    );
    assert_eq!(snapshot.participants[0].parameters, p.parameters);
    assert_eq!(f.count("operation_logs"), audit_before);
    assert_eq!(f.app.panel.changes().version(), panel_before);
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    assert_eq!(
        support::files(&f.app.environment.paths().data_root().join("logs")),
        logs_before
    );
    assert!(support::files(&f.root()).is_empty());
    // 失败结果按快照缓存，不自动回源或重试。
    let cached = f.app.iq_test.precheck(next.snapshot_id).await.unwrap();
    assert_eq!(cached.precheck.status, result.precheck.status);
    assert_eq!(cached.precheck.message, result.precheck.message);
    assert_eq!(cached.precheck.checked_at_ms, result.precheck.checked_at_ms);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].head.contains(&format!("Bearer {SECRET}")));
    assert!(requests[1].head.contains(&format!("Bearer {NEW_KEY}")));
    for request in requests {
        assert!(request
            .head
            .contains(&format!("x-vendor-authorization: Bearer {OLD_AUTH}")));
        assert!(request
            .head
            .contains(&format!("x-client-token: {OLD_HEADER}")));
        assert!(!request.head.contains(NEW_AUTH));
        assert!(!request.head.contains(NEW_HEADER));
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["vendor_feature"], true);
        assert_eq!(body["max_tokens"], 48000);
    }
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn upstream_error_formats_produce_bounded_redacted_summaries_without_response_dumps() {
    let f = Fixture::new();
    let long_message = format!(
        "reasoning is not supported; {SECRET}\n{}",
        "长错误内容 ".repeat(1500)
    );
    let cases = [
        (Response::raw(422, "application/json", json!({"detail":"reasoning is not supported","extra":"private-response-dump"}).to_string()), "reasoning is not supported"),
        (Response::raw(400, "text/plain; charset=utf-8", format!("reasoning is not supported; {SECRET}\nprivate-response-dump")), "reasoning is not supported"),
        (Response::sse(&[("data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"reasoning is not supported\"}}}\n\n", 0)]), "reasoning is not supported"),
        (Response::raw(400, "application/json", json!({"error":{"message":long_message},"extra":"private-response-dump"}).to_string()), "reasoning is not supported"),
        (Response::raw(400, "application/json", json!({"error":{"message":format!("{} {SECRET}", "oversized".repeat(9000))}}).to_string()), "未提供可识别的错误说明"),
        (Response::raw(502, "text/html", format!("<html>private-response-dump {SECRET}</html>")), "未提供可识别的错误说明"),
    ];
    let server = Server::new(cases.iter().map(|(response, _)| response.clone()).collect());
    let site = f.site(&server.url, Protocol::OpenaiResponses, &["model"], true);
    let before = f.count("operation_logs");
    for (_, expected) in cases {
        let p = f
            .participant(&site, 0, json!({"reasoning":{"effort":"high"}}))
            .await;
        let result = f.app.iq_test.precheck(p.snapshot_id).await.unwrap();
        assert_eq!(result.precheck.status, PrecheckStatus::Unavailable);
        assert!(
            result.precheck.message.contains(expected),
            "{}",
            result.precheck.message
        );
        assert!(result.precheck.message.chars().count() <= 512);
        assert!(!result.precheck.message.contains(SECRET));
        assert!(!result.precheck.message.contains("private-response-dump"));
    }
    assert_eq!(f.count("operation_logs"), before);
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    assert!(support::files(&f.root()).is_empty());
    f.app.iq_test.stop().await;
}
