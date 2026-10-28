use super::*;

#[tokio::test]
async fn catalog_parses_pages_and_rejects_failure_without_partial_results() {
    let server = Server::new(vec![
        Response::json(
            json!({"data":[{"id":"remote-a"},{"id":"remote-a"}],"has_more":true,"last_id":"remote-a"}),
        ),
        Response::json(json!({"data":[{"id":"remote-b"}]})),
    ]);
    let client = ModelClient::default();
    let catalog = CatalogRequest {
        target: target(&server.url, Protocol::Anthropic),
        endpoint: "/v1/models".into(),
    };
    assert_eq!(
        client
            .list_models(
                &catalog,
                ExecutionOptions::default(),
                &Cancellation::default()
            )
            .await
            .unwrap(),
        vec!["remote-a", "remote-b"]
    );
    assert!(server.requests()[1]
        .head
        .starts_with("GET /v1/models?after_id=remote-a"));
    for response in [
        Response::raw(401, "application/json", "{\"secret\":\"do-not-leak\"}"),
        Response::json(json!({"data":[{"wrong":"id"}]})),
        Response::raw(200, "text/html", "not json"),
        Response::raw(200, "application/json", " ".repeat(2 * 1024 * 1024 + 1)),
    ] {
        let server = Server::new(vec![response]);
        let input = CatalogRequest {
            target: target(&server.url, Protocol::OpenaiCompatible),
            endpoint: "/v1/models".into(),
        };
        assert!(client
            .list_models(
                &input,
                ExecutionOptions::default(),
                &Cancellation::default()
            )
            .await
            .is_err());
    }
    let repeated = Response::json(json!({"data":["a"],"has_more":true,"last_id":"a"}));
    let server = Server::new(vec![repeated.clone(), repeated]);
    let input = CatalogRequest {
        target: target(&server.url, Protocol::Anthropic),
        endpoint: "/v1/models".into(),
    };
    assert!(client
        .list_models(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default()
        )
        .await
        .is_err());
}
#[tokio::test]
async fn catalog_accepts_supported_shapes_and_has_an_overall_deadline() {
    let client = ModelClient::default();
    for response in [
        json!(["a",{"name":"b"}]),
        json!({"models":["a",{"id":"b"}]}),
    ] {
        let server = Server::new(vec![Response::json(response)]);
        let input = CatalogRequest {
            target: target(&server.url, Protocol::OpenaiCompatible),
            endpoint: "/models".into(),
        };
        assert_eq!(
            client
                .list_models(
                    &input,
                    ExecutionOptions::default(),
                    &Cancellation::default()
                )
                .await
                .unwrap(),
            vec!["a", "b"]
        );
    }
    let server = Server::new(vec![
        Response::json(json!([])).delayed(Duration::from_secs(1))
    ]);
    let input = CatalogRequest {
        target: target(&server.url, Protocol::OpenaiCompatible),
        endpoint: "/models".into(),
    };
    let options = ExecutionOptions {
        total_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_millis(150),
    };
    assert!(matches!(
        client
            .list_models(&input, options, &Cancellation::default())
            .await,
        Err(ClientError::Timeout)
    ));
}
