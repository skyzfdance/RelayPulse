use super::*;

#[test]
fn endpoint_paths_are_the_literal_base_plus_endpoint() {
    for custom in [false, true] {
        for base in ["https://example.test/v1", "https://example.test/v1/"] {
            for path in ["responses", "/responses"] {
                assert_eq!(
                    endpoint(base, path, custom).unwrap().as_str(),
                    format!("{base}{path}")
                );
            }
        }
        assert_eq!(
            endpoint("https://aitogether.site/usage", "v1/res", custom)
                .unwrap()
                .as_str(),
            "https://aitogether.site/usagev1/res"
        );
        assert_eq!(
            endpoint("https://example.test/base/", "//models", custom)
                .unwrap()
                .as_str(),
            "https://example.test/base///models"
        );
        assert_eq!(
            endpoint(
                "https://example.test/base",
                "https://other.test/models",
                custom
            )
            .unwrap()
            .as_str(),
            "https://example.test/basehttps://other.test/models"
        );
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

#[test]
fn addresses_that_require_rewriting_are_rejected_instead_of_corrected() {
    for custom in [false, true] {
        for path in [
            "/../models",
            "/./models",
            "/%2e%2e/models",
            "/a\\models",
            "/a\tmodels",
            "/a b",
        ] {
            assert!(
                endpoint("https://example.test/base", path, custom).is_err(),
                "{path}"
            );
        }
    }
}

#[tokio::test]
async fn catalog_probes_and_generation_send_literal_paths_over_http() {
    for (base_path, path, expected) in [
        ("/usage", "v1/res", "/usagev1/res"),
        ("/usage", "/v1/res", "/usage/v1/res"),
        ("/usage/", "v1/res", "/usage/v1/res"),
        ("/usage/", "/v1/res", "/usage//v1/res"),
        ("/v1", "/v1/responses", "/v1/v1/responses"),
    ] {
        let server = Server::new(vec![
            Response::json(json!({"data":[{"id":"model"}]})),
            test_support::success(Protocol::OpenaiCompatible),
            test_support::success(Protocol::OpenaiCompatible),
            test_support::success(Protocol::OpenaiCompatible),
        ]);
        let client = ModelClient::default();
        let base = format!("{}{base_path}", server.url);
        let catalog = CatalogRequest {
            target: target(&base, Protocol::OpenaiCompatible),
            endpoint: path.into(),
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
        let mut capture = AddressCapture::default();
        for mode in [EndpointMode::Relative, EndpointMode::SameOriginAppend] {
            let mut input = request(&base, Protocol::OpenaiCompatible);
            input.endpoint = path.into();
            input.endpoint_mode = mode;
            assert!(
                client
                    .check_once(
                        &input,
                        ExecutionOptions::default(),
                        &Cancellation::default(),
                        Some(&mut capture)
                    )
                    .await
                    .unwrap()
                    .success
            );
        }
        let input = GenerationRequest {
            target: catalog.target,
            model: "model".into(),
            endpoint: path.into(),
            prompt: "test literal address".into(),
            parameters: Default::default(),
        };
        let result = client
            .generate_once(
                &input,
                GenerationOptions {
                    output_timeout: Some(Duration::from_secs(2)),
                },
                &Cancellation::default(),
                &mut capture,
            )
            .await
            .unwrap();
        assert_eq!(result.failure, None);
        let sent = server.requests();
        assert_eq!(sent.len(), 4);
        assert!(sent[0].head.starts_with(&format!("GET {expected} ")));
        for request in &sent[1..] {
            assert!(
                request.head.starts_with(&format!("POST {expected} ")),
                "{}",
                request.head
            );
        }
        assert_eq!(capture.0, vec![format!("{}{expected}", server.url); 3]);
    }
}

#[derive(Default)]
struct AddressCapture(Vec<String>);
impl CaptureSink for AddressCapture {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> futures_util::future::BoxFuture<'a, Result<(), CaptureError>> {
        Box::pin(async move {
            if let CaptureEvent::Started { url, .. } = event {
                self.0.push(url.into());
            }
            Ok(())
        })
    }
}
