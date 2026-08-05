use crate::domain::{
    AuthPreset, DiscoveryStatus, EntityId, HttpMethod, LatestModelStatus, LifecycleState, Model,
    ModelSource, ProbeCapture, ProbeMode, ProbePolicy, ProbeResult, ProbeRunKind, ProbeStatus,
    ProbeTimingPolicy, ResponseMode, Site, SiteProtocol, TimestampMs, TokenUsage,
    REQUEST_CAPTURE_LIMIT_BYTES, RESPONSE_CAPTURE_LIMIT_BYTES,
};

pub fn id(value: &str) -> EntityId {
    EntityId::parse(value).expect("fixture id is valid")
}

pub fn timestamp(value: i64) -> TimestampMs {
    TimestampMs::new(value).expect("fixture timestamp is valid")
}

pub fn site(value: &str, sort_order: u32) -> Site {
    Site {
        id: id(value),
        name: format!("Site {value}"),
        protocol: SiteProtocol::OpenAiCompatible,
        base_url: "https://example.test/v1".to_owned(),
        credential_ref: None,
        enabled: true,
        sort_order,
        models_method: HttpMethod::Get,
        models_endpoint: "/models".to_owned(),
        probe_method: HttpMethod::Post,
        probe_endpoint: "/chat/completions".to_owned(),
        content_type: "application/json".to_owned(),
        auth_preset: AuthPreset::Bearer,
        stream_enabled: true,
        timing: ProbeTimingPolicy::default(),
        lifecycle_state: LifecycleState::Ready,
        created_at: timestamp(1),
        updated_at: timestamp(1),
    }
}

pub fn site_policy(site_id: &EntityId) -> ProbePolicy {
    ProbePolicy {
        id: id(&format!("policy-{site_id}")),
        site_id: site_id.clone(),
        model_id: None,
        mode: ProbeMode::DisabledThinking,
        max_output_tokens: 8,
        prompt: "OK".to_owned(),
        custom_endpoint: None,
        custom_method: None,
        custom_stream_enabled: None,
        created_at: timestamp(1),
        updated_at: timestamp(1),
    }
}

pub fn model(value: &str, site_id: &EntityId, sort_order: u32) -> Model {
    Model {
        id: id(value),
        site_id: site_id.clone(),
        model_key: value.to_owned(),
        display_name: Some(format!("Model {value}")),
        source: ModelSource::Manual,
        discovery_status: DiscoveryStatus::Available,
        enabled: true,
        sort_order,
        created_at: timestamp(2),
        updated_at: timestamp(2),
    }
}

pub fn probe_result(
    value: &str,
    site_id: &EntityId,
    model_id: &EntityId,
    started_at: i64,
    status: ProbeStatus,
) -> ProbeResult {
    ProbeResult {
        id: id(value),
        site_id: site_id.clone(),
        model_id: model_id.clone(),
        run_kind: ProbeRunKind::Manual,
        request_method: HttpMethod::Post,
        endpoint: "https://example.test/v1/chat/completions".to_owned(),
        probe_mode: ProbeMode::DisabledThinking,
        started_at: timestamp(started_at),
        finished_at: Some(timestamp(started_at + 100)),
        status,
        response_headers_ms: Some(20),
        first_token_ms: Some(40),
        total_time_ms: Some(100),
        http_status: Some(200),
        provider_model: Some(model_id.to_string()),
        finish_reason: Some("stop".to_owned()),
        error_code: None,
        error_message: None,
        token_usage: TokenUsage {
            input_tokens: Some(5),
            output_tokens: Some(1),
            total_tokens: Some(6),
            reported: true,
        },
        ended_normally: true,
        retry_count: 0,
        request_id: Some(format!("request-{value}")),
        revision: 0,
    }
}

pub fn capture(probe_result_id: &EntityId) -> ProbeCapture {
    ProbeCapture {
        probe_result_id: probe_result_id.clone(),
        request_headers_json: "[]".to_owned(),
        request_body: "{}".to_owned(),
        request_captured_bytes: 2,
        request_limit_bytes: REQUEST_CAPTURE_LIMIT_BYTES,
        request_truncated: false,
        response_mode: Some(ResponseMode::Stream),
        response_content_type: Some("text/event-stream".to_owned()),
        response_headers_json: "[]".to_owned(),
        response_body: "data: [DONE]\n\n".to_owned(),
        response_chunks_json: "[]".to_owned(),
        response_captured_bytes: 14,
        response_limit_bytes: RESPONSE_CAPTURE_LIMIT_BYTES,
        response_truncated: false,
    }
}

pub fn latest(result: &ProbeResult) -> LatestModelStatus {
    LatestModelStatus {
        model_id: result.model_id.clone(),
        probe_result_id: Some(result.id.clone()),
        status: result.status,
        consecutive_failures: u16::from(result.status != ProbeStatus::Healthy),
        consecutive_successes: u16::from(result.status == ProbeStatus::Healthy),
        next_probe_at: Some(timestamp(result.started_at.as_i64() + 600_000)),
        revision: 0,
        updated_at: result.finished_at.expect("fixture probe is finished"),
    }
}
