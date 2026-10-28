use super::health::*;
use crate::modules::sites::{Connection, Protocol};
use crate::shared::model_client::CheckFailure;
use crate::shared::model_client::CheckResult;
#[test]
fn stable_health_requires_thresholds_and_slow_success_counts_toward_recovery() {
    let connection = Connection::for_protocol(Protocol::OpenaiCompatible);
    let success = CheckResult {
        success: true,
        status_code: Some(200),
        duration_ms: 10,
        first_token_ms: None,
        failure: None,
        metadata: Default::default(),
    };
    let failure = CheckResult {
        success: false,
        status_code: Some(500),
        duration_ms: 20,
        first_token_ms: None,
        failure: Some(CheckFailure::Http),
        metadata: Default::default(),
    };
    let mut state = HealthState::default();
    for i in 1..=2 {
        state = advance(&state, &failure, &connection, i);
        assert_eq!(state.health, Health::Muted);
    }
    state = advance(&state, &success, &connection, 3);
    assert_eq!(state.health, Health::Healthy);
    assert_eq!(state.failures, 0);
    for i in 4..=5 {
        state = advance(&state, &failure, &connection, i);
        assert_eq!(state.health, Health::Healthy);
    }
    state = advance(&state, &failure, &connection, 6);
    assert_eq!(state.health, Health::Error);
    state = advance(&state, &success, &connection, 7);
    assert_eq!(state.health, Health::Error);
    state = advance(&state, &failure, &connection, 8);
    assert_eq!(state.successes, 0);
    let slow = CheckResult {
        duration_ms: 30_000,
        ..success.clone()
    };
    state = advance(&state, &slow, &connection, 9);
    assert_eq!(state.health, Health::Error);
    state = advance(&state, &slow, &connection, 10);
    assert_eq!(state.health, Health::Warning);
    state = advance(&state, &success, &connection, 11);
    assert_eq!(state.health, Health::Healthy);
    let first = CheckResult {
        first_token_ms: Some(5000),
        ..success
    };
    assert_eq!(
        advance(&state, &first, &connection, 12).health,
        Health::Warning
    );
    assert!(!due(1000, 2, 2999));
    assert!(due(1000, 2, 3000));
}

#[test]
fn summary_prioritizes_errors_then_slowness_then_unchecked_models() {
    use Health::*;
    for (states, health, text) in [
        (vec![], Muted, "没有启用模型"),
        (vec![Healthy, Healthy], Healthy, "全部正常"),
        (vec![Healthy, Muted], Muted, "等待探测"),
        (vec![Muted, Warning, Healthy], Warning, "响应偏慢"),
        (vec![Warning, Error, Muted, Error], Error, "2 个异常"),
    ] {
        let summary = summarize(states);
        assert_eq!(summary.health, health);
        assert_eq!(summary.status_text, text);
    }
}
