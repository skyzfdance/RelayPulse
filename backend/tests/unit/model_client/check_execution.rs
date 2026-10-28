use super::*;

#[tokio::test]
async fn streams_measure_real_first_content_not_headers_or_role_and_inject_credentials() {
    let server =
        Server::new(vec![Response::sse(&[
        ("data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\r\n\r\n", 0),
        ("data: {\"choices\":[{\"delta\":{\"content\":\"O", 30),
        ("K\"}}]}\r\n\r\n", 20),
        ("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n", 20),
    ])]);
    let mut input = request(&server.url, Protocol::OpenaiCompatible);
    input.target.headers.push(RequestHeader {
        key: "X-Empty".into(),
        value: "".into(),
    });
    let result = check(&ModelClient::default(), &input).await;
    assert!(result.success, "{result:?}");
    assert!(result.first_token_ms.unwrap() >= 40);
    assert!(result.duration_ms >= result.first_token_ms.unwrap());
    assert_eq!(result.status_code, Some(200));
    let received = &server.requests()[0];
    assert!(received.head.starts_with("POST /v1/chat/completions"));
    assert!(received
        .head
        .to_lowercase()
        .contains("authorization: bearer local-test-secret"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&received.body).unwrap()["model"],
        "remote-name"
    );
    let user_agent = received
        .head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("user-agent")
                .then(|| value.trim())
        })
        .expect("default User-Agent is sent");
    assert!(user_agent.starts_with(concat!("RelayPulse/", env!("CARGO_PKG_VERSION"), " (")));
    assert!(user_agent.ends_with(')'));
    let (system, architecture) = user_agent
        .split_once(" (")
        .unwrap()
        .1
        .trim_end_matches(')')
        .split_once("; ")
        .unwrap();
    assert!(system
        .split_once(' ')
        .is_some_and(|(name, version)| !name.is_empty() && !version.is_empty()));
    assert!(!architecture.is_empty());
    #[cfg(target_os = "macos")]
    assert!(system.starts_with("macOS "));
    #[cfg(target_os = "windows")]
    assert!(system.starts_with("Windows "));
    #[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
    assert_eq!(architecture, "arm");
}
#[tokio::test]
async fn anthropic_responses_json_and_sse_success_and_incomplete_failure() {
    for (protocol,response) in [
        (Protocol::Anthropic,Response::sse(&[("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"OK\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",0)])),
        (Protocol::OpenaiResponses,Response::sse(&[("data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\ndata: {\"type\":\"response.completed\"}\n\n",0)])),
        (Protocol::Anthropic,Response::json(json!({"content":[{"type":"text","text":"OK"}],"stop_reason":"end_turn"}))),
        (Protocol::OpenaiResponses,Response::json(json!({"status":"completed","output":[{"content":[{"type":"output_text","text":"OK"}]}]}))),
        (Protocol::OpenaiCompatible,Response::json(json!({"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}))),
    ] {
        let streaming = response.content_type == "text/event-stream"; let server = Server::new(vec![response]);
        let result = check(&ModelClient::default(), &request(&server.url, protocol)).await;
        assert!(result.success, "{protocol:?}: {result:?}"); assert_eq!(result.first_token_ms.is_some(), streaming);
        if protocol == Protocol::Anthropic { let head=server.requests()[0].head.to_lowercase(); assert!(head.contains("x-api-key: local-test-secret")); assert!(head.contains("anthropic-version: 2023-06-01")); }
    }
    for response in [
        Response::raw(200, "text/html", "<html>login</html>"),
        Response::sse(&[("data: [DONE]\n\n", 0)]),
        Response::sse(&[(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
            0,
        )]),
        Response::json(json!({"error":{"message":"must-not-be-exposed"}})),
    ] {
        let server = Server::new(vec![response]);
        let result = check(
            &ModelClient::default(),
            &request(&server.url, Protocol::OpenaiCompatible),
        )
        .await;
        assert!(!result.success);
        assert!(!result.message().contains("must-not-be-exposed"));
    }
}
#[tokio::test]
async fn custom_http_failure_timeout_and_redirect_do_not_fabricate_success() {
    let server = Server::new(vec![
        Response::raw(204, "text/plain", ""),
        Response::raw(503, "text/plain", "upstream secret"),
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_secs(1)),
    ]);
    let mut input = request(&server.url, Protocol::OpenaiCompatible);
    input.endpoint = "/custom".into();
    input.endpoint_mode = EndpointMode::SameOriginAppend;
    input.body = CheckBody::Custom { parameters: vec![] };
    let client = ModelClient::default();
    let success = check(&client, &input).await;
    assert!(success.success);
    assert_eq!(success.first_token_ms, None);
    let failed = check(&client, &input).await;
    assert!(!failed.success);
    assert_eq!(failed.status_code, Some(503));
    let options = ExecutionOptions {
        total_timeout: Duration::from_secs(2),
        idle_timeout: Duration::from_millis(150),
    };
    let timeout = client
        .check_once(&input, options, &Cancellation::default(), None)
        .await
        .unwrap();
    assert_eq!(timeout.failure, Some(CheckFailure::Timeout));
    assert!(timeout.duration_ms >= 100);
    assert_eq!(timeout.status_code, Some(200));
    let destination = Server::new(vec![]);
    let mut redirect = Response::raw(302, "text/plain", "");
    redirect.headers.push(("Location", destination.url.clone()));
    let origin = Server::new(vec![redirect]);
    input.target.base_url = origin.url.clone();
    let redirected = check(&client, &input).await;
    assert_eq!(redirected.status_code, Some(302));
    assert!(!redirected.success);
    assert!(destination.requests().is_empty());
}
#[tokio::test]
async fn header_timeout_and_connection_failure_return_real_failure_with_no_status() {
    let mut delayed = Response::raw(200, "text/plain", "OK");
    delayed.header_delay = Duration::from_secs(2);
    let server = Server::new(vec![delayed]);
    let client = ModelClient::default();
    let options = ExecutionOptions {
        total_timeout: Duration::from_millis(300),
        idle_timeout: Duration::from_secs(1),
    };
    let result = client
        .check_once(
            &request(&server.url, Protocol::CustomHttp),
            options,
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(CheckFailure::Timeout));
    assert_eq!(result.status_code, None);
    assert!(result.duration_ms >= 250);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let result = check(&client, &request(&url, Protocol::CustomHttp)).await;
    assert_eq!(result.failure, Some(CheckFailure::Network));
    assert_eq!(result.status_code, None);
}
#[tokio::test]
async fn cancelling_one_request_does_not_cancel_other_calls_or_fake_a_result() {
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "slow").delayed(Duration::from_secs(1)),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let client = ModelClient::default();
    let cancelled = Cancellation::default();
    let signal = cancelled.clone();
    let input = request(&server.url, Protocol::CustomHttp);
    let other = input.clone();
    let worker = client.clone();
    let task = tokio::spawn(async move {
        worker
            .check_once(&input, ExecutionOptions::default(), &signal, None)
            .await
    });
    server.wait_requests(1).await;
    cancelled.cancel();
    assert!(matches!(task.await.unwrap(), Err(ClientError::Cancelled)));
    assert!(check(&client, &other).await.success);
    assert!(matches!(
        client
            .check_once(&other, ExecutionOptions::default(), &cancelled, None)
            .await,
        Err(ClientError::Cancelled)
    ));
    assert_eq!(server.requests().len(), 2);
}
#[tokio::test]
async fn invalid_input_is_a_call_error_without_sending_an_http_request() {
    let server = Server::new(vec![]);
    let mut input = request(&server.url, Protocol::CustomHttp);
    input.body = CheckBody::Custom {
        parameters: vec![Parameter::new(
            "__proto__.polluted",
            "true",
            ParameterType::Boolean,
        )],
    };
    assert!(matches!(
        ModelClient::default()
            .check_once(
                &input,
                ExecutionOptions::default(),
                &Cancellation::default(),
                None
            )
            .await,
        Err(ClientError::Invalid(_))
    ));
    assert!(server.requests().is_empty());
}
