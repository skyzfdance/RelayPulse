use super::*;

#[tokio::test]
async fn trial_uses_unsaved_draft_and_formal_probes_respect_model_not_site_enabled() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(503, "text/plain", "secret response"),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f.site(&server.url, &["enabled", "disabled"]);
    let site = f.store.get(&id).unwrap();
    f.store
        .set_model_enabled(&id, &site.models[1].id, false)
        .unwrap();
    let before = f.execute(Operation::Get(id.clone())).await;
    let test = f
        .execute(Operation::Test(
            id.clone(),
            site.models[0].id.clone(),
            CustomRequest {
                version: 2,
                endpoint: "/draft".into(),
                parameters: vec![],
            },
        ))
        .await;
    assert_eq!(test["simulated"], false);
    assert_eq!(test["success"], false);
    assert_eq!(test["statusCode"], 503);
    assert_eq!(f.count(), 0);
    assert_eq!(f.execute(Operation::Get(id.clone())).await, before);
    let result = f.execute(Operation::Run(id.clone(), None)).await;
    assert_eq!(result["models"][0]["health"], "healthy");
    assert_eq!(result["models"][1]["health"], "muted");
    assert_eq!(f.count(), 1);
    let result = f
        .execute(Operation::Run(id.clone(), Some(site.models[1].id.clone())))
        .await;
    assert_eq!(result["models"][1]["health"], "healthy");
    assert_eq!(f.count(), 2);
    assert!(server.requests()[0].head.starts_with("POST /draft"));
    assert!(server.requests()[1].head.starts_with("POST /probe"));
    let exported = String::from_utf8(f.diagnostics.package().unwrap()).unwrap();
    for secret in ["secret-only-in-vault", "secret response", "/draft"] {
        assert!(!exported.contains(secret));
    }
}
#[tokio::test]
async fn real_failures_and_slow_successes_drive_persisted_health_and_recovery() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_millis(1050)),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f.site(&server.url, &["m"]);
    let site = f.store.get(&id).unwrap();
    let mut connection = site.connection;
    connection.failure_threshold = 2;
    connection.recovery_threshold = 2;
    connection.slow_total_seconds = 1;
    f.store.save_connection(&id, &connection).unwrap();
    for (index, expected) in ["muted", "error", "error", "warning", "healthy"]
        .iter()
        .enumerate()
    {
        let result = f.execute(Operation::Run(id.clone(), None)).await;
        assert_eq!(result["models"][0]["health"], *expected);
        assert_eq!(f.execute(Operation::Get(id.clone())).await, result);
        assert_eq!(f.count(), index as i64 + 1);
        if *expected == "warning" {
            assert!(result["models"][0]["durationMs"].as_u64().unwrap() >= 1000);
            assert_eq!(result["models"][0]["firstTokenMs"], Value::Null);
        }
    }
    let states = f
        .monitoring
        .states(site.models.iter().map(|m| m.id.clone()).collect())
        .await
        .unwrap();
    assert_eq!(states[&site.models[0].id].failures, 0);
    assert_eq!(states[&site.models[0].id].successes, 3);
    let outcomes: Vec<String> = f
        .database
        .with_connection(|c| {
            c.prepare("SELECT result_kind FROM probe_records ORDER BY rowid")?
                .query_map([], |r| r.get(0))?
                .collect()
        })
        .unwrap();
    assert_eq!(
        outcomes,
        ["failed", "failed", "succeeded", "slow", "succeeded"]
    );
}
