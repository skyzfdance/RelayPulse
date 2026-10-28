use super::*;
use std::fs;

fn row(f: &Fixture) -> Value {
    let text: String = f.database.with_connection(|c| c.query_row(
        "SELECT json_object('id',id,'siteName',site_name,'modelName',model_name,'protocol',protocol,
         'source',run_kind,'endpoint',request_endpoint,'policy',json(policy_json),'started',started_at_ms,
         'finished',finished_at_ms,'kind',result_kind,'requestId',request_id,'input',input_tokens,
         'output',output_tokens,'total',total_tokens,'complete',body_complete,'capture',capture_enabled)
         FROM probe_records ORDER BY rowid DESC LIMIT 1",[],|r|r.get(0))).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn events(f: &Fixture, id: &str) -> Vec<Value> {
    let path = f
        ._directory
        .path()
        .join("com.relaypulse.app.dev/probe-records")
        .join(id)
        .join("response.jsonl");
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn real_probe_saves_frozen_metadata_and_redacted_exact_request_with_provider_usage() {
    let f = Fixture::new();
    let mut response = Response::sse(&[
        ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n",0),
        ("data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":2,\"total_tokens\":12}}}\n\n",10),
    ]);
    response
        .headers
        .push(("x-request-id", "provider-secret-only-in-vault-id".into()));
    let server = Server::new(vec![response]);
    let id = f
        .store
        .create(
            &CreateSite {
                name: "original".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: server.url.clone(),
                secret: "secret-only-in-vault".into(),
                enabled: false,
            },
            &["remote-name".into()],
        )
        .unwrap();
    let mut policy = ProbePolicy::for_protocol(Protocol::OpenaiResponses);
    policy.parameters.extend(serde_json::from_value::<Vec<crate::modules::sites::Parameter>>(json!([
            {"key":"metadata.options","value":"{\"api_key\":\"request-only-secret\",\"note\":\"secret-only-in-vault\",\"keep\":true}","type":"json"}
        ])).unwrap());
    f.store.save_policy(&id, None, Some(&policy), None).unwrap();
    f.execute(Operation::Run(id.clone(), None)).await;
    let saved = row(&f);
    assert_eq!(saved["siteName"], "original");
    assert_eq!(saved["modelName"], "remote-name");
    assert_eq!(saved["protocol"], "openai-responses");
    assert_eq!(saved["source"], "manual");
    assert_eq!(saved["endpoint"], format!("{}/v1/responses", server.url));
    assert_eq!(saved["policy"]["source"], "site_default");
    assert_eq!(saved["input"], 10);
    assert_eq!(saved["output"], 2);
    assert_eq!(saved["total"], 12);
    assert_eq!(saved["requestId"], "provider-[REDACTED]-id");
    assert_eq!(saved["complete"], 1);
    assert_eq!(saved["capture"], 1);
    assert!(saved["finished"].as_u64().unwrap() >= saved["started"].as_u64().unwrap());
    let log = events(&f, saved["id"].as_str().unwrap());
    let started = log
        .iter()
        .find(|event| event["event"] == "request_started")
        .unwrap();
    let actual: Value = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(started["method"], "POST");
    assert_eq!(started["body"]["model"], actual["model"]);
    assert_eq!(started["body"]["reasoning"], actual["reasoning"]);
    assert_eq!(
        started["body"]["max_output_tokens"],
        actual["max_output_tokens"]
    );
    assert_eq!(
        actual["metadata"]["options"]["api_key"],
        "request-only-secret"
    );
    assert_eq!(
        started["body"]["metadata"]["options"]["api_key"],
        "[REDACTED]"
    );
    assert_eq!(started["body"]["metadata"]["options"]["note"], "[REDACTED]");
    assert_eq!(log.last().unwrap()["metadata"]["usage"]["totalTokens"], 12);
    for text in [
        saved.to_string(),
        serde_json::to_string(&log).unwrap(),
        String::from_utf8(f.diagnostics.package().unwrap()).unwrap(),
    ] {
        assert!(!text.contains("secret-only-in-vault"));
        assert!(!text.contains("request-only-secret"));
    }
    f.execute(Operation::Save(
        id,
        SiteDraft {
            name: "renamed".into(),
            protocol: Protocol::OpenaiResponses,
            base_url: server.url.clone(),
            credential_action: CredentialAction::Keep,
            secret: String::new(),
        },
    ))
    .await;
    assert_eq!(row(&f), saved);
}

#[tokio::test]
async fn custom_formal_request_keeps_its_own_endpoint_and_body_but_trial_creates_nothing() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "trial"),
        Response::raw(200, "text/plain", "formal"),
    ]);
    let id = f.site(&server.url, &["image-model"]);
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    let custom = CustomRequest {
        version: 2,
        endpoint:"/images/generations?api_key=query-only-secret#fragment-only-secret".into(),
        parameters:serde_json::from_value(json!([
            {"key":"model","value":"","type":"string","binding":"model"},
            {"key":"prompt","value":"tiny image","type":"string"},{"key":"n","value":"1","type":"number"}
        ])).unwrap(),
    };
    f.execute(Operation::Test(id.clone(), model.clone(), custom.clone()))
        .await;
    assert_eq!(f.count(), 0);
    let data = f._directory.path().join("com.relaypulse.app.dev");
    assert!(!data.join("probe-records").exists());
    f.execute(Operation::Policy(
        id.clone(),
        Some(model),
        None,
        Some(custom),
    ))
    .await;
    f.execute(Operation::Run(id, None)).await;
    let saved = row(&f);
    assert_eq!(saved["policy"]["source"], "custom_request");
    assert_eq!(saved["policy"]["maxTokens"], Value::Null);
    assert_eq!(saved["input"], Value::Null);
    assert_eq!(saved["total"], Value::Null);
    assert_eq!(
        saved["endpoint"],
        format!("{}/images/generations", server.url)
    );
    let log = events(&f, saved["id"].as_str().unwrap());
    let started = log
        .iter()
        .find(|e| e["event"] == "request_started")
        .unwrap();
    assert_eq!(
        started["body"],
        json!({"model":"image-model","prompt":"tiny image","n":1})
    );
    assert_eq!(
        log.iter()
            .filter(|e| e["event"] == "request_started")
            .count(),
        1
    );
    assert!(!serde_json::to_string(&log)
        .unwrap()
        .contains("query-only-secret"));
    assert!(!saved.to_string().contains("query-only-secret"));
    assert!(server.requests()[1]
        .head
        .starts_with("POST /images/generations?api_key=query-only-secret"));
}

#[tokio::test]
async fn cleanup_expires_idle_probe_history_and_files_without_touching_iq_replay_or_health() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let id = f.site(&server.url, &["model"]);
    let before = f.execute(Operation::Run(id.clone(), None)).await;
    let saved = row(&f);
    let data = f._directory.path().join("com.relaypulse.app.dev");
    let directory = data
        .join("probe-records")
        .join(saved["id"].as_str().unwrap());
    let iq = data.join("iq-test/run/stream.1.sse");
    fs::create_dir_all(iq.parent().unwrap()).unwrap();
    fs::write(&iq, b"data: iq-output\n\n").unwrap();
    f.database
        .with_connection(|c| c.execute("UPDATE probe_records SET finished_at_ms=0", []))
        .unwrap();
    f.monitoring.cleanup_records().await;
    assert_eq!(f.count(), 0);
    assert!(!directory.exists());
    assert_eq!(fs::read(iq).unwrap(), b"data: iq-output\n\n");
    assert_eq!(f.execute(Operation::Get(id)).await, before);
}
