use super::*;

#[tokio::test]
async fn saved_input_can_be_checked_without_monitoring_and_freezes_credentials() {
    use crate::shared::model_client::{Cancellation, ExecutionOptions};
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f.site(&server.url, &["m"]);
    let snapshot = f.sites.execution_snapshot(id.clone()).await.unwrap();
    let model_id = snapshot.site.models[0].id.clone();
    let input = snapshot.resolve_probe_input(&model_id).unwrap();
    f.sites
        .mutate(
            id.clone(),
            SiteMutation::Basic(SiteDraft {
                name: "updated".into(),
                protocol: Protocol::CustomHttp,
                base_url: server.url.clone(),
                credential_action: CredentialAction::Replace,
                secret: "rotated-secret".into(),
            }),
        )
        .await
        .unwrap();
    let before_log: Value = serde_json::from_slice(&f.diagnostics.package().unwrap()).unwrap();
    let check = ModelClient::default()
        .check_once(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap();
    assert!(check.success);
    assert_eq!(f.count(), 0);
    assert!(f
        .monitoring
        .states(vec![model_id.clone()])
        .await
        .unwrap()
        .is_empty());
    let after_log: Value = serde_json::from_slice(&f.diagnostics.package().unwrap()).unwrap();
    assert_eq!(after_log["events"], before_log["events"]);
    assert_eq!(after_log["logStatistics"], before_log["logStatistics"]);
    assert!(server.requests()[0]
        .head
        .to_lowercase()
        .contains("authorization: bearer secret-only-in-vault"));
    f.monitoring.run(id, Some(model_id)).await.unwrap();
    assert_eq!(f.count(), 1);
    assert!(server.requests()[1]
        .head
        .to_lowercase()
        .contains("authorization: bearer rotated-secret"));
    assert_eq!(server.requests()[0].body, server.requests()[1].body);
}
#[tokio::test]
async fn resolved_inputs_preserve_default_model_override_and_custom_request_semantics() {
    use crate::shared::model_client::{Cancellation, ExecutionOptions};
    let f = Fixture::new();
    let client = ModelClient::default();
    for (protocol, expected) in [
        (
            Protocol::OpenaiCompatible,
            json!({"model":"remote","messages":[{"role":"user","content":"只回复 OK"}],"stream":true,"max_tokens":8,"reasoning_effort":"low"}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"model":"remote","input":"只回复 OK","stream":true,"reasoning":{"effort":"low"}}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"remote","messages":[{"role":"user","content":"只回复 OK"}],"stream":true,"max_tokens":4096,"thinking":{"type":"adaptive"},"output_config":{"effort":"low"}}),
        ),
        (Protocol::CustomHttp, json!({"model":"remote"})),
    ] {
        let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
        let id = f
            .store
            .create(
                &CreateSite {
                    name: "contract".into(),
                    protocol,
                    base_url: server.url.clone(),
                    models_path: "/models".into(),
                    probe_path: match protocol {
                        Protocol::OpenaiCompatible => "/v1/chat/completions",
                        Protocol::OpenaiResponses => "/v1/responses",
                        Protocol::Anthropic => "/v1/messages",
                        Protocol::CustomHttp => "/probe",
                    }
                    .into(),
                    secret: "fixture".into(),
                    enabled: false,
                },
                &["remote".into()],
            )
            .unwrap();
        let snapshot = f.sites.execution_snapshot(id).await.unwrap();
        let input = snapshot
            .resolve_probe_input(&snapshot.site.models[0].id)
            .unwrap();
        client
            .check_once(
                &input,
                ExecutionOptions::default(),
                &Cancellation::default(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&server.requests()[0].body).unwrap(),
            expected
        );
    }
    let response = Response::sse(&[
        (
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n",
            5,
        ),
        ("data: {\"type\":\"response.completed\"}\n\n", 0),
    ]);
    let server = Server::new(vec![response.clone(), response]);
    let id = f
        .store
        .create(
            &CreateSite {
                name: "override".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: format!("{}/base", server.url),
                models_path: "/models".into(),
                probe_path: "/v1/responses".into(),
                secret: "fixture".into(),
                enabled: true,
            },
            &["remote".into()],
        )
        .unwrap();
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    let policy = ProbePolicy {
        headers: vec![],
        version: 2,
        parameters: serde_json::from_value(json!([
            {"key":"model","type":"string","value":"","binding":"model"},
            {"key":"input","type":"string","value":"model prompt"},
            {"key":"max_output_tokens","type":"number","value":"64"},
            {"key":"reasoning","type":"object","value":"{}"},
            {"key":"reasoning.budget","type":"number","value":"10"},
            {"key":"reasoning.effort","type":"string","value":"high"}
        ]))
        .unwrap(),
    };
    f.sites
        .mutate(
            id.clone(),
            SiteMutation::Policy(Some(model.clone()), Some(policy), None),
        )
        .await
        .unwrap();
    let snapshot = f.sites.execution_snapshot(id.clone()).await.unwrap();
    client
        .check_once(
            &snapshot.resolve_probe_input(&model).unwrap(),
            ExecutionOptions::default(),
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(body["input"], "model prompt");
    assert_eq!(body["max_output_tokens"], 64);
    assert_eq!(body["reasoning"], json!({"budget":10,"effort":"high"}));
    f.sites
        .mutate(
            id.clone(),
            SiteMutation::Policy(
                Some(model.clone()),
                None,
                Some(CustomRequest {
                    headers: vec![],
                    version: 2,
                    endpoint: "custom".into(),
                    parameters: vec![],
                }),
            ),
        )
        .await
        .unwrap();
    let snapshot = f.sites.execution_snapshot(id).await.unwrap();
    let result = client
        .check_once(
            &snapshot.resolve_probe_input(&model).unwrap(),
            ExecutionOptions::default(),
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.first_token_ms.is_some());
    assert!(server.requests()[1].head.starts_with("POST /basecustom "));
    assert_eq!(
        serde_json::from_slice::<Value>(&server.requests()[1].body).unwrap(),
        json!({})
    );
}

#[tokio::test]
async fn independent_requests_keep_protocol_parsing_for_trial_and_formal_probes() {
    use crate::shared::model_client::{CheckBody, Parameter, ParameterType};
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let response = match protocol {
            Protocol::OpenaiCompatible => Response::sse(&[
                ("data: {\"choices\":[{\"delta\":{\"content\":\"OK\"}}]}\n\n", 5),
                ("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n", 0),
                ("data: [DONE]\n\n", 0),
            ]),
            Protocol::OpenaiResponses => Response::sse(&[
                ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n", 5),
                ("data: {\"type\":\"response.completed\"}\n\n", 0),
            ]),
            Protocol::Anthropic => Response::sse(&[
                ("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"OK\"}}\n\n", 5),
                ("data: {\"type\":\"message_stop\"}\n\n", 0),
            ]),
            Protocol::CustomHttp => Response::raw(200, "text/plain", "OK"),
        };
        let f = Fixture::new();
        let server = Server::new(vec![
            response.clone(),
            response,
            Response::raw(200, "text/plain", "invalid protocol response"),
        ]);
        let id = f
            .store
            .create(
                &CreateSite {
                    name: "independent".into(),
                    protocol,
                    base_url: format!("{}/usage", server.url),
                    models_path: "/models".into(),
                    probe_path: "/default".into(),
                    secret: "fixture".into(),
                    enabled: false,
                },
                &["remote".into()],
            )
            .unwrap();
        let original = f.store.get(&id).unwrap();
        let model = original.models[0].id.clone();
        let custom = CustomRequest {
            version: 2,
            endpoint: "v1/res".into(),
            headers: vec![Parameter::new(
                "x-independent",
                "visible",
                ParameterType::String,
            )],
            parameters: vec![
                Parameter::model(),
                Parameter::new("custom", "body", ParameterType::String),
            ],
        };
        let (trial, _) = f
            .sites
            .test_draft(id.clone(), model.clone(), custom.clone())
            .await
            .unwrap();
        assert!(trial.success, "{protocol:?}");
        assert_eq!(
            trial.first_token_ms.is_some(),
            protocol != Protocol::CustomHttp
        );
        assert_eq!(f.store.get(&id).unwrap(), original);
        assert_eq!(f.count(), 0);
        f.store
            .save_policy(&id, Some(&model), None, Some(&custom))
            .unwrap();
        let snapshot = f.sites.execution_snapshot(id.clone()).await.unwrap();
        let resolved = snapshot.resolve_probe_input(&model).unwrap();
        assert_eq!(
            matches!(resolved.body, CheckBody::Custom { .. }),
            protocol == Protocol::CustomHttp
        );
        f.execute(Operation::Run(id.clone(), Some(model.clone())))
            .await;
        let states = f.monitoring.states(vec![model.clone()]).await.unwrap();
        assert_eq!(
            states[&model].first_token_ms.is_some(),
            protocol != Protocol::CustomHttp
        );
        assert_eq!(f.count(), 1);
        let (invalid, _) = f.sites.test_draft(id, model, custom).await.unwrap();
        assert_eq!(invalid.success, protocol == Protocol::CustomHttp);
        if protocol != Protocol::CustomHttp {
            assert_eq!(
                invalid.failure,
                Some(crate::shared::model_client::CheckFailure::InvalidResponse)
            );
        }
        for request in server.requests() {
            assert!(request.head.starts_with("POST /usagev1/res "));
            assert!(request
                .head
                .to_lowercase()
                .contains("x-independent: visible"));
            assert_eq!(
                serde_json::from_slice::<Value>(&request.body).unwrap(),
                json!({"model":"remote","custom":"body"})
            );
        }
    }
}
#[tokio::test]
async fn saved_parameter_table_controls_real_requests_inheritance_and_model_replacement() {
    let f = Fixture::new();
    let response = Response::json(
        json!({"status":"completed","output":[{"content":[{"type":"output_text","text":"OK"}]}]}),
    );
    let server = Server::new(vec![response.clone(), response.clone(), response]);
    let id = f
        .store
        .create(
            &CreateSite {
                name: "table".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: server.url.clone(),
                models_path: "/models".into(),
                probe_path: "/v1/responses".into(),
                secret: "fixture".into(),
                enabled: false,
            },
            &["remote".into()],
        )
        .unwrap();
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    let policy: ProbePolicy = serde_json::from_value(json!({"version":2,"parameters":[
        {"key":"model","value":"","type":"string","enabled":true,"binding":"model"},
        {"key":"input","value":"只回复 OK","type":"string","enabled":true},
        {"key":"stream","value":"true","type":"boolean","enabled":true},
        {"key":"max_tokens","value":"8","type":"number","enabled":true},
        {"key":"reasoning","value":"{}","type":"object","enabled":true},
        {"key":"reasoning.effort","value":"low","type":"string","enabled":true},
        {"key":"max_output_tokens","value":"invalid","type":"number","enabled":false}
    ]}))
    .unwrap();
    f.store.save_policy(&id, None, Some(&policy), None).unwrap();
    assert_eq!(f.store.get(&id).unwrap().policy, policy);
    f.execute(Operation::Run(id.clone(), Some(model.clone())))
        .await;
    assert_eq!(
        serde_json::from_slice::<Value>(&server.requests()[0].body).unwrap(),
        json!({"model":"remote","input":"只回复 OK","stream":true,"max_tokens":8,"reasoning":{"effort":"low"}})
    );
    let mut own: ProbePolicy = serde_json::from_value(json!({"version":2,"parameters":[
        {"key":"reasoning.effort","value":"high","type":"string","enabled":false},
        {"key":"reasoning","value":"{}","type":"object","enabled":true}
    ]}))
    .unwrap();
    f.store
        .save_policy(&id, Some(&model), Some(&own), None)
        .unwrap();
    f.execute(Operation::Run(id.clone(), Some(model.clone())))
        .await;
    assert_eq!(
        serde_json::from_slice::<Value>(&server.requests()[1].body).unwrap(),
        json!({"reasoning":{}})
    );
    own.parameters[0].enabled = true;
    own.parameters[0].kind = crate::modules::sites::ParameterType::Json;
    own.parameters[0].value = "{".into();
    own.parameters[1].enabled = false;
    f.store
        .save_policy(&id, Some(&model), Some(&own), None)
        .unwrap();
    f.execute(Operation::Run(id, Some(model))).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&server.requests()[2].body).unwrap(),
        json!({})
    );
}

#[tokio::test]
async fn catalog_pagination_and_default_model_custom_probes_use_separate_header_tables() {
    use crate::shared::model_client::{
        Cancellation, ExecutionOptions, Parameter, ParameterType, RequestHeader,
    };
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"first"}],"has_more":true,"last_id":"first"})),
        Response::json(json!({"data":[{"id":"second"}],"has_more":false})),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f.site(&format!("{}/usage", server.url), &["one", "two", "three"]);
    let site = f.store.get(&id).unwrap();
    let mut connection = site.connection.clone();
    connection.models_path = "models".into();
    connection.headers = vec![RequestHeader {
        key: "x-catalog".into(),
        value: "catalog".into(),
    }];
    f.store.save_connection(&id, &connection).unwrap();
    let header = |value: &str| Parameter::new("x-probe.route", value, ParameterType::String);
    let mut policy = site.policy.clone();
    policy.headers = vec![header("default")];
    let mut disabled = header("disabled");
    disabled.key = "x-disabled".into();
    disabled.enabled = false;
    policy.headers.push(disabled.clone());
    f.store.save_policy(&id, None, Some(&policy), None).unwrap();
    policy.headers = vec![header("model")];
    f.store
        .save_policy(&id, Some(&site.models[1].id), Some(&policy), None)
        .unwrap();
    let custom = CustomRequest {
        version: 2,
        headers: vec![header("custom")],
        endpoint: "custom".into(),
        parameters: vec![],
    };
    f.store
        .save_policy(&id, Some(&site.models[2].id), None, Some(&custom))
        .unwrap();
    f.service
        .execute(Operation::Sync(
            id.clone(),
            vec!["one".into(), "two".into(), "three".into()],
        ))
        .await
        .unwrap();
    let snapshot = f.sites.execution_snapshot(id.clone()).await.unwrap();
    assert_eq!(snapshot.site.policy.headers[1], disabled);
    for model in &site.models {
        ModelClient::default()
            .check_once(
                &snapshot.resolve_probe_input(&model.id).unwrap(),
                ExecutionOptions::default(),
                &Cancellation::default(),
                None,
            )
            .await
            .unwrap();
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert!(requests[0].head.starts_with("GET /usagemodels "));
    assert!(requests[1]
        .head
        .starts_with("GET /usagemodels?after_id=first "));
    for request in &requests[..2] {
        assert!(request.head.contains("x-catalog: catalog"));
        assert!(!request.head.contains("x-probe.route:"));
    }
    for (request, expected) in requests[2..].iter().zip(["default", "model", "custom"]) {
        assert!(request.head.contains(&format!("x-probe.route: {expected}")));
        assert!(!request.head.contains("x-catalog:"));
        assert!(!request.head.contains("x-disabled:"));
    }
    assert!(requests[4].head.starts_with("POST /usagecustom "));
}
