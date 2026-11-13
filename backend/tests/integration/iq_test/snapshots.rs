use super::*;

#[tokio::test]
async fn iq_prechecks_generation_and_saved_metadata_use_literal_addresses() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        for (base_path, endpoint, path) in [
            ("/usage", "v1/res", "/usagev1/res"),
            ("/usage", "/v1/res", "/usage/v1/res"),
            ("/usage/", "v1/res", "/usage/v1/res"),
            ("/usage/", "/v1/res", "/usage//v1/res"),
        ] {
            let f = Fixture::new();
            let server = Server::new(vec![success(protocol); 2]);
            let site = f.site(
                &format!("{}{base_path}", server.url),
                protocol,
                &["model"],
                true,
            );
            let mut connection = site.connection.clone();
            connection.probe_path = endpoint.into();
            f.sites.save_connection(&site.id, &connection).unwrap();
            let participant = f.available(&site, 0, json!({})).await;
            let expected = format!("{}{path}", server.url);
            assert_eq!(participant.endpoint, expected);
            let run = f.start(&[f.question("literal address").await]).await;
            let run = f.terminal(&run.id).await;
            assert_eq!(run.entries[0].status, EntryStatus::Completed);
            assert_eq!(run.participants[0].endpoint, expected);
            assert_eq!(
                meta(&f.root(), &run.entries[0], 1)["participant"]["endpoint"],
                expected
            );
            let requests = server.requests();
            assert_eq!(requests.len(), 2);
            assert!(requests
                .iter()
                .all(|request| request.head.starts_with(&format!("POST {path} "))));
            f.app.iq_test.stop().await;
        }
    }
}
use crate::{
    application::sites::Operation,
    modules::sites::{CredentialAction, CustomRequest, SiteDraft},
    shared::model_client::{Parameter, ParameterType, RequestHeader},
};

#[tokio::test]
async fn candidate_health_is_read_from_formal_state_but_never_controls_or_changes_iq_prechecks() {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 2]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], false);
    assert_eq!(
        f.app.iq_test.snapshot().await.unwrap().candidates[0].health,
        "muted"
    );
    f.database
        .with_connection(|c| {
            c.execute(
                "INSERT INTO probe_states VALUES (?1,'error',NULL,42,3,0,1)",
                [&site.models[0].id],
            )
        })
        .unwrap();
    assert_eq!(
        f.app.iq_test.snapshot().await.unwrap().candidates[0].health,
        "error"
    );
    let p = f.available(&site, 0, json!({})).await;
    assert_eq!(p.candidate.health, "error");
    let state: String = f
        .database
        .with_connection(|c| {
            c.query_row(
                "SELECT health FROM probe_states WHERE model_id=?1",
                [&site.models[0].id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "error");
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_states SET health='healthy' WHERE model_id=?1",
                [&site.models[0].id],
            )
        })
        .unwrap();
    let snapshot = f.app.iq_test.snapshot().await.unwrap();
    assert_eq!(snapshot.candidates[0].health, "healthy");
    assert_eq!(
        snapshot.participants[0].candidate.health, "error",
        "已建立的参与者展示快照同样不自动回源刷新"
    );
    let run = f
        .start(&[f.question("precheck availability is independent").await])
        .await;
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.participants[0].candidate.health, "error");
    assert_eq!(f.count("probe_records"), 0);
    assert_eq!(f.count("probe_states"), 1);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn candidate_rules_and_execution_snapshots_survive_rotation_source_deletion_and_question_edits(
) {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 3]);
    let site = f.site(
        &server.url,
        Protocol::OpenaiCompatible,
        &["enabled", "disabled"],
        false,
    );
    f.sites
        .set_model_enabled(&site.id, &site.models[1].id, false)
        .unwrap();
    let mut connection = site.connection.clone();
    connection.probe_path = "/frozen-endpoint?token=must-not-appear".into();
    connection.headers.push(RequestHeader {
        key: "x-snapshot".into(),
        value: "original".into(),
    });
    f.sites.save_connection(&site.id, &connection).unwrap();
    f.sites
        .save_policy(
            &site.id,
            Some(&site.models[0].id),
            None,
            Some(&CustomRequest {
                headers: vec![],
                version: 2,
                endpoint: "/probe-template-only".into(),
                parameters: vec![Parameter::new(
                    "probe_secret_parameter",
                    "do-not-inherit",
                    ParameterType::String,
                )],
            }),
        )
        .unwrap();
    let candidates = f.app.iq_test.snapshot().await.unwrap().candidates;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].model_name, "enabled");
    let p = f
        .participant(
            &site,
            0,
            json!({"reasoning_effort":"medium","temperature":0.35,"metadata":{"iq":"independent"}}),
        )
        .await;
    assert_eq!(p.preset, "高 (high)");
    assert!(!p.endpoint.contains("token="));
    f.app
        .sites
        .execute(Operation::Save(
            site.id.clone(),
            SiteDraft {
                name: "renamed".into(),
                protocol: site.protocol,
                base_url: server.url.clone(),
                credential_action: CredentialAction::Replace,
                secret: "rotated-execution-key".into(),
            },
        ))
        .await
        .unwrap();
    connection.probe_path = "/new-endpoint".into();
    connection.headers[0].value = "replacement".into();
    f.app
        .sites
        .execute(Operation::Connection(site.id.clone(), connection))
        .await
        .unwrap();
    let audit_before = f.count("operation_logs");
    let panel_before = f.app.panel.changes().version();
    let logs_before = support::files(&f.app.environment.paths().data_root().join("logs"));
    let precheck = f.app.iq_test.precheck(p.snapshot_id.clone()).await.unwrap();
    assert_eq!(precheck.precheck.status, PrecheckStatus::Available);
    assert_eq!(f.count("operation_logs"), audit_before);
    assert_eq!(f.app.panel.changes().version(), panel_before);
    assert_eq!(
        support::files(&f.app.environment.paths().data_root().join("logs")),
        logs_before
    );
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    let first = &server.requests()[0];
    assert!(first
        .head
        .starts_with("POST /frozen-endpoint?token=must-not-appear "));
    assert!(first.head.contains(&format!("Bearer {SECRET}")));
    assert!(!first.head.to_lowercase().contains("x-snapshot:"));
    let body: Value = serde_json::from_slice(&first.body).unwrap();
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("probe_secret_parameter").is_none());
    assert_eq!(body["reasoning_effort"], "medium");
    assert_eq!(body["temperature"], 0.35);
    assert_eq!(body["metadata"], json!({"iq":"independent"}));
    // 当前快照可在源站点已删除后开始，后续题目也不回源。
    f.app
        .sites
        .execute(Operation::Delete(site.id.clone()))
        .await
        .unwrap();
    assert!(f
        .app
        .iq_test
        .snapshot()
        .await
        .unwrap()
        .candidates
        .is_empty());
    let q1 = f.question("当题普通 user 内容一").await;
    let q2 = f.question("当题普通 user 内容二").await;
    let run = f.start(&[q1.clone(), q2.clone()]).await;
    f.app.iq_test.delete_question(q1.id.clone()).await.unwrap();
    f.app
        .iq_test
        .save_question(
            IqQuestionInput {
                content: "later edit".into(),
                reference_answer: None,
                enabled: false,
            },
            Some(q2.id.clone()),
        )
        .await
        .unwrap();
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.summary.call_result.value, CallResultValue::AllSuccess);
    assert_eq!(run.questions[0].content, q1.content);
    assert_eq!(run.questions[1].content, q2.content);
    assert_eq!(run.participants[0].snapshot_id, p.snapshot_id);
    for (index, request) in server.requests().iter().enumerate().skip(1) {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert!(request.head.contains(&format!("Bearer {SECRET}")));
        assert!(!request.head.contains("rotated-execution-key"));
        assert!(request
            .head
            .starts_with("POST /frozen-endpoint?token=must-not-appear "));
        assert_eq!(
            body["messages"],
            json!([{"role":"user","content":if index == 1 { &q1.content } else { &q2.content }}])
        );
        assert_eq!(body["model"], "enabled");
        assert_eq!(body["stream"], true);
        assert_eq!(body["reasoning_effort"], "medium");
        assert_eq!(body["temperature"], 0.35);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("probe_secret_parameter").is_none());
    }
    let projection = serde_json::to_string(&run).unwrap();
    for secret in [SECRET, "rotated-execution-key", "must-not-appear"] {
        assert!(!projection.contains(secret));
    }
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn only_manual_refresh_of_a_failed_snapshot_reacquires_current_credentials_and_preserves_iq_values(
) {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(503, "application/json", "{\"error\":\"unavailable\"}"),
        success(Protocol::OpenaiResponses),
    ]);
    let site = f.site(&server.url, Protocol::OpenaiResponses, &["model"], false);
    let p = f
        .participant(
            &site,
            0,
            json!({"reasoning":{"effort":"low"},"temperature":0.1,"metadata":{"note":SECRET}}),
        )
        .await;
    assert!(matches!(
        f.app.iq_test.refresh_participant(p.id.clone()).await,
        Err(IqError::Validation { .. })
    ));
    let before = f.count("operation_logs");
    assert_eq!(
        f.app
            .iq_test
            .precheck(p.snapshot_id.clone())
            .await
            .unwrap()
            .precheck
            .status,
        PrecheckStatus::Unavailable
    );
    assert_eq!(f.count("operation_logs"), before);
    f.app
        .sites
        .execute(Operation::Save(
            site.id.clone(),
            SiteDraft {
                name: "new source".into(),
                protocol: site.protocol,
                base_url: server.url.clone(),
                credential_action: CredentialAction::Replace,
                secret: "latest-authorized-key".into(),
            },
        ))
        .await
        .unwrap();
    let replacement = f
        .app
        .iq_test
        .refresh_participant(p.id.clone())
        .await
        .unwrap();
    assert_eq!(replacement.id, p.id);
    assert_ne!(replacement.snapshot_id, p.snapshot_id);
    assert_eq!(replacement.parameters, p.parameters);
    assert_eq!(replacement.preset, p.preset);
    assert_eq!(replacement.precheck.status, PrecheckStatus::Checking);
    assert!(matches!(
        f.app.iq_test.precheck(p.snapshot_id.clone()).await,
        Err(IqError::NotFound)
    ));
    let ready = f
        .app
        .iq_test
        .precheck(replacement.snapshot_id.clone())
        .await
        .unwrap();
    assert_eq!(ready.snapshot_id, replacement.snapshot_id);
    assert_eq!(ready.precheck.status, PrecheckStatus::Available);
    assert!(server.requests()[1]
        .head
        .contains("Bearer latest-authorized-key"));
    // 相同快照预检完成后再次查询不创建另一次请求。
    assert_eq!(
        f.app
            .iq_test
            .precheck(replacement.snapshot_id)
            .await
            .unwrap()
            .precheck
            .status,
        PrecheckStatus::Available
    );
    assert_eq!(server.requests().len(), 2);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn removed_snapshot_late_precheck_cannot_overwrite_a_new_snapshot_and_missing_key_is_a_silent_result(
) {
    let f = Fixture::new();
    let mut slow = success(Protocol::OpenaiCompatible);
    slow.header_delay = Duration::from_secs(2);
    let server = Server::new(vec![slow, success(Protocol::OpenaiCompatible)]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    let old = f.participant(&site, 0, json!({})).await;
    let task = tokio::spawn({
        let service = f.app.iq_test.clone();
        let id = old.snapshot_id.clone();
        async move { service.precheck(id).await.unwrap() }
    });
    server.wait_requests(1).await;
    f.app
        .iq_test
        .remove_participant(old.id.clone())
        .await
        .unwrap();
    let new = f.available(&site, 0, json!({})).await;
    let late = task.await.unwrap();
    assert_eq!(late.snapshot_id, old.snapshot_id);
    assert_eq!(
        f.app.iq_test.snapshot().await.unwrap().participants[0].snapshot_id,
        new.snapshot_id
    );
    assert_eq!(
        f.app.iq_test.snapshot().await.unwrap().participants[0]
            .precheck
            .status,
        PrecheckStatus::Available
    );
    f.database
        .with_connection(|c| c.execute("DELETE FROM site_credentials WHERE site_id=?1", [&site.id]))
        .unwrap();
    let no_key = f.participant(&site, 0, json!({})).await;
    let before = f.count("operation_logs");
    assert_eq!(
        f.app
            .iq_test
            .precheck(no_key.snapshot_id)
            .await
            .unwrap()
            .precheck
            .status,
        PrecheckStatus::Unavailable
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(f.count("operation_logs"), before);
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn backend_rejects_reserved_fields_unsafe_paths_and_wrong_parameter_types_before_http() {
    let f = Fixture::new();
    let server = Server::new(vec![]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    let model_key = format!("{}/{}", site.id, site.models[0].id);
    for parameters in [
        json!({"model":"override"}),
        json!({"stream":false}),
        json!({"messages":[]}),
        json!({"prompt":"override"}),
        json!({"input":[]}),
        json!({"system":"override"}),
        json!({"thinking":{"constructor":{"x":1}}}),
        json!({"__proto__":{}}),
        json!({"options.__proto__":1}),
        json!({"temperature":"0.3"}),
        json!({"max_tokens":0}),
        json!({"reasoning":{"effort":true}}),
        json!({"thinking":{"budget_tokens":"32000"}}),
    ] {
        assert!(
            matches!(
                f.app
                    .iq_test
                    .create_participant(IqParticipantInput {
                        headers: vec![],
                        model_key: model_key.clone(),
                        preset: "高 (high)".into(),
                        parameters: parameters.as_object().unwrap().clone()
                    })
                    .await,
                Err(IqError::Validation {
                    field: "input.parameters",
                    ..
                })
            ),
            "{parameters}"
        );
    }
    assert!(server.requests().is_empty());
    assert!(f
        .app
        .iq_test
        .snapshot()
        .await
        .unwrap()
        .participants
        .is_empty());
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn independent_headers_are_frozen_for_precheck_and_generation_and_redacted_in_history() {
    use crate::shared::model_client::{Parameter, ParameterType};
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 2]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], false);
    let headers = vec![
        Parameter::new("x-iq-route", "frozen", ParameterType::String),
        Parameter::new("x-client-token", "private-iq-token", ParameterType::String),
        Parameter::new("x-auth", "private-x-auth", ParameterType::String),
    ];
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
    assert_eq!(p.headers[0].value, "frozen");
    assert_eq!(p.headers[1].value, "[REDACTED]");
    assert_eq!(p.headers[2].value, "[REDACTED]");
    let mut policy = site.policy.clone();
    policy.headers = vec![Parameter::new(
        "x-iq-route",
        "source-changed",
        ParameterType::String,
    )];
    f.sites
        .save_policy(&site.id, None, Some(&policy), None)
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
    let question = f.question("test frozen headers").await;
    let run = f.start(&[question]).await;
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    for request in server.requests() {
        assert!(request.head.contains("x-iq-route: frozen"));
        assert!(request.head.contains("x-client-token: private-iq-token"));
        assert!(request.head.contains("x-auth: private-x-auth"));
        assert!(!request.head.contains("source-changed"));
    }
    assert!(!serde_json::to_string(&run)
        .unwrap()
        .contains("private-iq-token"));
    for (file, bytes) in support::files(&f.root()) {
        if file
            .extension()
            .is_some_and(|ext| ext == "json" || ext == "jsonl")
        {
            assert!(!String::from_utf8_lossy(&bytes).contains("private-iq-token"));
            assert!(!String::from_utf8_lossy(&bytes).contains("private-x-auth"));
        }
    }
    f.app.iq_test.stop().await;
}
