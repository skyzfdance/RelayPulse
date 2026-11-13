use super::*;

fn input(base_url: String) -> CreateSite {
    CreateSite {
        name: "new site".into(),
        protocol: Protocol::OpenaiResponses,
        base_url,
        models_path: "/models".into(),
        probe_path: "/responses".into(),
        secret: "creation-only-in-vault".into(),
        enabled: false,
    }
}

#[tokio::test]
async fn creation_uses_user_endpoints_and_protocol_specific_requests() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![
            Response::json(json!(["remote"])),
            crate::shared::model_client::test_support::success(protocol),
        ]);
        let mut draft = input(format!("{}/usage", server.url));
        draft.protocol = protocol;
        draft.models_path = "catalog".into();
        draft.probe_path = "generate".into();
        let id = f
            .execute(Operation::Create(draft))
            .await
            .as_str()
            .unwrap()
            .to_owned();
        let site = f.store.get(&id).unwrap();
        assert_eq!(site.connection.models_path, "catalog");
        assert_eq!(site.connection.probe_path, "generate");
        f.execute(Operation::Run(id, None)).await;
        let sent = server.requests();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].head.starts_with("GET /usagecatalog "));
        assert!(sent[1].head.starts_with("POST /usagegenerate "));
        for request in sent {
            let headers = request.head.to_ascii_lowercase();
            if protocol == Protocol::Anthropic {
                assert!(headers.contains("x-api-key: creation-only-in-vault"));
                assert!(headers.contains("anthropic-version:"));
            } else {
                assert!(headers.contains("authorization: bearer creation-only-in-vault"));
            }
        }
    }
}

#[tokio::test]
async fn missing_catalog_endpoint_skips_discovery_and_allows_manual_models() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![]);
        let mut draft = input(server.url.clone());
        draft.protocol = protocol;
        draft.models_path.clear();
        let id = f
            .execute(Operation::Create(draft))
            .await
            .as_str()
            .unwrap()
            .to_owned();
        let site = f.store.get(&id).unwrap();
        assert!(site.models.is_empty());
        assert!(site.connection.models_path.is_empty());
        assert_eq!(site.connection.probe_path, "/responses");
        f.execute(Operation::AddModel(id.clone(), "manual".into()))
            .await;
        let connection = f.store.get(&id).unwrap().connection;
        f.execute(Operation::Connection(id.clone(), connection))
            .await;
        assert!(matches!(
            f.service
                .execute(Operation::Sync(id.clone(), vec!["manual".into()]))
                .await,
            Err(SiteError::Config(_))
        ));
        assert_eq!(f.store.get(&id).unwrap().models[0].name, "manual");
        assert!(server.requests().is_empty());
    }
}

#[tokio::test]
async fn invalid_user_endpoints_are_rejected_before_discovery_or_persistence() {
    let f = Fixture::new();
    let server = Server::new(vec![]);
    for (models, probe) in [
        (" /models", "/probe"),
        ("/models", ""),
        ("/models", "/../probe"),
    ] {
        let mut draft = input(server.url.clone());
        draft.models_path = models.into();
        draft.probe_path = probe.into();
        assert!(f.service.execute(Operation::Create(draft)).await.is_err());
    }
    assert!(server.requests().is_empty());
    assert!(f.store.list().unwrap().is_empty());
}

#[tokio::test]
async fn catalog_failures_still_create_sites_that_can_be_configured_and_synced() {
    for response in [
        Response::raw(401, "text/plain", "unauthorized"),
        Response::raw(403, "text/plain", "forbidden"),
        Response::raw(404, "text/plain", "not found"),
        Response::raw(429, "text/plain", "rate limited"),
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(200, "text/html", "not a catalog"),
        Response::json(json!({"data":[{"id":"partial"},{"missing":"id"}]})),
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![response, Response::json(json!(["remote"]))]);
        let base = format!("{}/gateway/v1", server.url);
        let id = f
            .execute(Operation::Create(input(base.clone())))
            .await
            .as_str()
            .unwrap()
            .to_owned();
        let site = f.store.get(&id).unwrap();
        assert_eq!(site.base_url, base);
        assert!(site.models.is_empty());
        assert!(site.credential_configured);
        assert!(!site.enabled);
        assert_eq!(site.connection.models_path, "/models");
        assert_eq!(site.connection.probe_path, "/responses");
        let mut connection = site.connection;
        connection.models_path = "/catalog".into();
        connection.probe_path = "/responses".into();
        f.execute(Operation::Connection(id.clone(), connection))
            .await;
        assert_eq!(
            f.execute(Operation::Sync(id.clone(), vec![])).await,
            json!({"added":["remote"],"removed":[]})
        );
        f.execute(Operation::Apply(
            id.clone(),
            ApplySync {
                models: vec![],
                added: vec!["remote".into()],
                removed: vec![],
            },
        ))
        .await;
        assert_eq!(f.store.get(&id).unwrap().models[0].name, "remote");
        let sent = server.requests();
        assert!(sent[0].head.starts_with("GET /gateway/v1/models "));
        assert!(sent[1].head.starts_with("GET /gateway/v1/catalog "));
        assert!(sent[1]
            .head
            .to_ascii_lowercase()
            .contains("authorization: bearer creation-only-in-vault"));
    }
}

#[tokio::test]
async fn successful_catalog_still_populates_new_sites() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::json(json!(["remote"]))]);
    let id = f
        .execute(Operation::Create(input(server.url.clone())))
        .await
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(f.store.get(&id).unwrap().models[0].name, "remote");
}

#[tokio::test]
async fn creation_populates_models_from_all_catalog_pages() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"first"}],"has_more":true,"last_id":"first"})),
        Response::json(json!({"data":[{"id":"second"}],"has_more":false})),
    ]);
    let base = format!("{}/usage/", server.url);
    let id = f
        .execute(Operation::Create(input(base.clone())))
        .await
        .as_str()
        .unwrap()
        .to_owned();
    let site = f.store.get(&id).unwrap();
    assert_eq!(site.base_url, base);
    assert_eq!(
        site.models
            .iter()
            .map(|model| model.name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    let sent = server.requests();
    assert_eq!(sent.len(), 2);
    assert!(sent[0].head.starts_with("GET /usage//models "));
    assert!(sent[1]
        .head
        .starts_with("GET /usage//models?after_id=first "));
}

#[tokio::test]
async fn unreachable_catalog_still_saves_the_site() {
    let f = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let id = f
        .execute(Operation::Create(input(url)))
        .await
        .as_str()
        .unwrap()
        .to_owned();
    assert!(f.store.get(&id).unwrap().models.is_empty());
}

#[tokio::test]
async fn catalog_timeout_still_saves_the_site() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::json(json!([])).delayed(Duration::from_secs(90))
    ]);
    let service = f.service.clone();
    let draft = input(server.url.clone());
    let pending = tokio::spawn(async move { service.execute(Operation::Create(draft)).await });
    server.wait_requests(1).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    let reply = pending.await.unwrap().unwrap();
    let id = serde_json::to_value(reply)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert!(f.store.get(&id).unwrap().models.is_empty());
}

#[tokio::test]
async fn invalid_input_and_storage_failure_do_not_report_creation_success() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(404, "text/plain", "not found")]);
    let mut invalid = input(server.url.clone());
    invalid.secret.clear();
    assert!(matches!(
        f.service.execute(Operation::Create(invalid)).await,
        Err(SiteError::Config(_))
    ));
    assert!(server.requests().is_empty());
    f.database
        .with_connection(|c| {
            c.execute_batch("CREATE TRIGGER reject_site BEFORE INSERT ON sites BEGIN SELECT RAISE(ABORT,'test'); END;")
        })
        .unwrap();
    assert!(matches!(
        f.service
            .execute(Operation::Create(input(server.url.clone())))
            .await,
        Err(SiteError::Config(_))
    ));
    assert!(f.store.list().unwrap().is_empty());
}

#[tokio::test]
async fn closing_during_catalog_discovery_does_not_create_a_site() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::json(json!([])).delayed(Duration::from_secs(1))
    ]);
    let service = f.service.clone();
    let draft = input(server.url.clone());
    let pending = tokio::spawn(async move { service.execute(Operation::Create(draft)).await });
    server.wait_requests(1).await;
    f.sites.close();
    assert!(matches!(
        pending.await.unwrap(),
        Err(SiteError::Task(TaskError::Closing))
    ));
    assert!(f.store.list().unwrap().is_empty());
}
