use super::*;

const MIB: usize = 1024 * 1024;

#[tokio::test]
async fn custom_responses_over_sixteen_mib_are_read_to_completion_without_text_parsing() {
    for declared_length in [false, true] {
        let mut response = Response::raw(200, "image/png", "");
        // 含非 UTF-8 字节；检查只消费正文，不解释图片或 JSON 内容。
        response.parts = vec![(Duration::ZERO, vec![0xff; 64 * 1024]); 17 * 16];
        response.parts.push((Duration::from_millis(60), vec![0]));
        if declared_length {
            response
                .headers
                .push(("Content-Length", (17 * MIB + 1).to_string()));
        }
        let server = Server::new(vec![response]);
        let result = check(
            &ModelClient::default(),
            &request(&server.url, Protocol::CustomHttp),
        )
        .await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.first_token_ms, None);
        assert!(
            result.duration_ms >= 50,
            "must wait for the final body bytes"
        );
    }
}

#[tokio::test]
async fn discarding_a_body_still_detects_truncation_idle_timeout_and_total_timeout() {
    let mut truncated = Response::raw(200, "application/octet-stream", "partial");
    truncated.headers.push(("Content-Length", MIB.to_string()));
    let mut idle = Response::raw(200, "application/octet-stream", "");
    idle.parts = vec![
        (Duration::ZERO, vec![0; 1024]),
        (Duration::from_secs(1), vec![1]),
    ];
    let mut slow = Response::raw(200, "application/octet-stream", "");
    slow.parts = vec![(Duration::from_millis(80), vec![0; 1024]); 5];
    let client = ModelClient::default();
    for (response, options, failure) in [
        (
            truncated,
            ExecutionOptions::default(),
            CheckFailure::Network,
        ),
        (
            idle,
            ExecutionOptions {
                total_timeout: Duration::from_secs(2),
                idle_timeout: Duration::from_millis(100),
            },
            CheckFailure::Timeout,
        ),
        (
            slow,
            ExecutionOptions {
                total_timeout: Duration::from_millis(200),
                idle_timeout: Duration::from_secs(1),
            },
            CheckFailure::Timeout,
        ),
    ] {
        let server = Server::new(vec![response]);
        let result = client
            .check_once(
                &request(&server.url, Protocol::CustomHttp),
                options,
                &Cancellation::default(),
                None,
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.failure, Some(failure));
        assert_eq!(result.first_token_ms, None);
    }
}

#[tokio::test]
async fn long_sse_responses_keep_parsing_completion_and_errors_after_sixteen_mib() {
    let tool_delta = format!(
        "data: {}\n\n",
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"x".repeat(64 * 1024)}}]}}]})
    ).into_bytes();
    for (tail, success) in [
        ("data: [DONE]\n\n", true),
        (
            "data: {\"error\":{\"message\":\"late failure\"}}\n\ndata: [DONE]\n\n",
            false,
        ),
        ("", false),
    ] {
        let mut response = Response::sse(&[(
            "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"}}]}\n\n",
            0,
        )]);
        response
            .parts
            .extend((0..17 * 16).map(|_| (Duration::ZERO, tool_delta.clone())));
        response
            .parts
            .push((Duration::ZERO, tail.as_bytes().to_vec()));
        let server = Server::new(vec![response]);
        let result = check(
            &ModelClient::default(),
            &request(&server.url, Protocol::OpenaiCompatible),
        )
        .await;
        assert_eq!(result.success, success, "{result:?}");
        assert_eq!(result.status_code, Some(200));
        assert!(result.first_token_ms.is_some());
        assert_eq!(
            result.failure,
            if success {
                None
            } else {
                Some(CheckFailure::InvalidResponse)
            }
        );
    }
}

#[tokio::test]
async fn complete_json_parsing_retains_its_buffer_limit() {
    for declared_length in [false, true] {
        let mut response = Response::raw(200, "application/json", "");
        // 有效的常规 JSON 响应；若删掉缓存保护，它本应被判断成功。
        response
            .parts
            .push((Duration::ZERO, b"{\"padding\":\"".to_vec()));
        response
            .parts
            .extend((0..17 * 16).map(|_| (Duration::ZERO, vec![b'x'; 64 * 1024])));
        response.parts.push((
            Duration::ZERO,
            br#"","choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#.to_vec(),
        ));
        if declared_length {
            let length: usize = response.parts.iter().map(|(_, part)| part.len()).sum();
            response
                .headers
                .push(("Content-Length", length.to_string()));
        }
        let server = Server::new(vec![response]);
        let result = check(
            &ModelClient::default(),
            &request(&server.url, Protocol::OpenaiCompatible),
        )
        .await;
        assert_eq!(result.failure, Some(CheckFailure::InvalidResponse));
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.first_token_ms, None);
    }
}

#[test]
fn sse_buffer_limits_apply_to_each_event_not_to_the_network_chunk() {
    let data = "x".repeat(600 * 1024);
    let wire = format!("data: {data}\r\n\r\ndata: {data}\r\n\r\n");
    assert!(wire.len() > MIB);
    let mut decoder = stream::Sse::default();
    assert_eq!(
        decoder.push(wire.as_bytes()).unwrap(),
        vec![data.clone(), data]
    );
    assert!(decoder.complete());
}

#[test]
fn sse_rejects_oversized_unfinished_lines_and_multiline_events() {
    let mut decoder = stream::Sse::default();
    decoder.push(&vec![b'x'; 600 * 1024]).unwrap();
    assert!(matches!(
        decoder.push(&vec![b'x'; 500 * 1024]),
        Err(ClientError::Response)
    ));

    let mut decoder = stream::Sse::default();
    let line = format!("data: {}\n", "x".repeat(600 * 1024));
    assert!(decoder.push(line.as_bytes()).unwrap().is_empty());
    assert!(matches!(
        decoder.push(line.as_bytes()),
        Err(ClientError::Response)
    ));
}
