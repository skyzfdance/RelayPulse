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
            json!({"model":"remote","input":"只回复 OK","stream":true,"max_output_tokens":8,"reasoning":{"effort":"low"}}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"remote","messages":[{"role":"user","content":"只回复 OK"}],"stream":true,"max_tokens":8}),
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
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f
        .store
        .create(
            &CreateSite {
                name: "override".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: format!("{}/base", server.url),
                secret: "fixture".into(),
                enabled: true,
            },
            &["remote".into()],
        )
        .unwrap();
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    let policy = ProbePolicy {
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
    assert_eq!(result.first_token_ms, None);
    assert!(server.requests()[1].head.starts_with("POST /base/custom"));
    assert_eq!(
        serde_json::from_slice::<Value>(&server.requests()[1].body).unwrap(),
        json!({})
    );
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
