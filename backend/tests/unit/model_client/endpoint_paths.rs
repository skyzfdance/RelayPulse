use super::*;

#[test]
fn endpoint_paths_preserve_base_prefix_and_normalize_joining_slashes() {
    for custom in [false, true] {
        for base in ["https://example.test/v1", "https://example.test/v1/"] {
            for path in ["responses", "/responses"] {
                assert_eq!(
                    endpoint(base, path, custom).unwrap().as_str(),
                    "https://example.test/v1/responses"
                );
            }
        }
        assert_eq!(
            endpoint(
                "https://example.test/gateway/v1",
                "/responses?region=test",
                custom
            )
            .unwrap()
            .as_str(),
            "https://example.test/gateway/v1/responses?region=test"
        );
        assert_eq!(
            endpoint("https://example.test", "/v1/models", custom)
                .unwrap()
                .as_str(),
            "https://example.test/v1/models"
        );
        assert_eq!(
            endpoint("https://example.test/v1", "/v1/responses", custom)
                .unwrap()
                .as_str(),
            "https://example.test/v1/v1/responses",
            "不猜测并删除用户显式配置的重复路径段"
        );
    }
}

#[tokio::test]
async fn catalog_and_both_probe_modes_send_the_base_path_prefix_over_http() {
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"model"}]})),
        test_support::success(Protocol::OpenaiCompatible),
        test_support::success(Protocol::OpenaiCompatible),
    ]);
    let client = ModelClient::default();
    let base = format!("{}/gateway/v1", server.url);
    let catalog = CatalogRequest {
        target: target(&base, Protocol::OpenaiCompatible),
        endpoint: "/models".into(),
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
        vec!["model"]
    );
    for mode in [EndpointMode::Relative, EndpointMode::SameOriginAppend] {
        let mut input = request(&base, Protocol::OpenaiCompatible);
        input.endpoint = "/chat/completions".into();
        input.endpoint_mode = mode;
        assert!(check(&client, &input).await.success);
    }
    let sent = server.requests();
    assert!(sent[0].head.starts_with("GET /gateway/v1/models "));
    for request in &sent[1..] {
        assert!(request
            .head
            .starts_with("POST /gateway/v1/chat/completions "));
    }
}
