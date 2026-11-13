use super::*;
use futures_util::future::BoxFuture;
use reqwest::header::HeaderMap;

#[derive(Default)]
struct Capture {
    headers: Vec<HeaderMap>,
}
impl CaptureSink for Capture {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, Result<(), CaptureError>> {
        Box::pin(async move {
            if let CaptureEvent::Started { headers, .. } = event {
                self.headers.push(headers.clone());
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn catalog_pages_probes_generation_and_capture_never_complete_provider_headers() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        for configured in [
            None,
            Some((false, "disabled-version")),
            Some((true, "user-version")),
            Some((true, "")),
        ] {
            let server = Server::new(vec![
                Response::json(json!({"data":["first"],"has_more":true,"last_id":"first"})),
                Response::json(json!({"data":["second"],"has_more":false})),
                test_support::success(protocol),
                test_support::success(protocol),
            ]);
            let mut values = vec![];
            if let Some((enabled, value)) = configured {
                let mut header = Parameter::new("Anthropic-Version", value, ParameterType::String);
                header.enabled = enabled;
                values.push(header);
            }
            let expected = configured.and_then(|(enabled, value)| enabled.then_some(value));
            let mut target = target(&server.url, protocol);
            target.headers = header_parameters(&values).unwrap();
            let client = ModelClient::default();
            let names = client
                .list_models(
                    &CatalogRequest {
                        target: target.clone(),
                        endpoint: "/models".into(),
                    },
                    ExecutionOptions::default(),
                    &Cancellation::default(),
                )
                .await
                .unwrap();
            assert_eq!(names, ["first", "second"]);
            let mut input = request(&server.url, protocol);
            input.target = target.clone();
            input.body = if protocol == Protocol::CustomHttp {
                CheckBody::Custom { parameters: vec![] }
            } else {
                CheckBody::Standard { parameters: vec![] }
            };
            let mut capture = Capture::default();
            assert!(
                client
                    .check_once(
                        &input,
                        ExecutionOptions::default(),
                        &Cancellation::default(),
                        Some(&mut capture),
                    )
                    .await
                    .unwrap()
                    .success
            );
            let generated = client
                .generate_once(
                    &GenerationRequest {
                        target,
                        model: "remote".into(),
                        endpoint: "/generate".into(),
                        prompt: "user question".into(),
                        parameters: Default::default(),
                    },
                    GenerationOptions {
                        output_timeout: Some(Duration::from_secs(2)),
                    },
                    &Cancellation::default(),
                    &mut capture,
                )
                .await
                .unwrap();
            assert_eq!(generated.failure, None);
            let requests = server.requests();
            assert_eq!(requests.len(), 4);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&requests[2].body).unwrap(),
                json!({})
            );
            for sent in &requests {
                let version = sent.head.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("anthropic-version")
                        .then(|| value.trim())
                });
                assert_eq!(version, expected);
                let credential = if protocol == Protocol::Anthropic {
                    "x-api-key: local-test-secret"
                } else {
                    "authorization: bearer local-test-secret"
                };
                assert!(sent.head.to_ascii_lowercase().contains(credential));
            }
            assert_eq!(capture.headers.len(), 2);
            for headers in capture.headers {
                assert_eq!(
                    headers
                        .get("anthropic-version")
                        .map(|value| value.to_str().unwrap()),
                    expected
                );
            }
        }
    }
}
