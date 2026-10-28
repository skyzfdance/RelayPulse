use crate::{modules::sites::Connection, shared::model_client::CheckResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Health {
    Healthy,
    Warning,
    Error,
    #[default]
    Muted,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HealthState {
    pub health: Health,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub failures: u32,
    pub successes: u32,
    pub finished_at_ms: u64,
}
pub(crate) struct HealthSummary {
    pub health: Health,
    pub status_text: String,
}

/// 只汇总调用方传入的参与监测模型；站点展示开关不参与健康判断。
pub(crate) fn summarize(states: impl IntoIterator<Item = Health>) -> HealthSummary {
    let states: Vec<_> = states.into_iter().collect();
    let errors = states.iter().filter(|h| **h == Health::Error).count();
    let (health, status_text) = if states.is_empty() {
        (Health::Muted, "没有启用模型".into())
    } else if errors > 0 {
        (Health::Error, format!("{errors} 个异常"))
    } else if states.contains(&Health::Warning) {
        (Health::Warning, "响应偏慢".into())
    } else if states.contains(&Health::Muted) {
        (Health::Muted, "等待探测".into())
    } else {
        (Health::Healthy, "全部正常".into())
    };
    HealthSummary {
        health,
        status_text,
    }
}

pub(crate) fn advance(
    previous: &HealthState,
    result: &CheckResult,
    connection: &Connection,
    finished_at_ms: u64,
) -> HealthState {
    let mut state = previous.clone();
    state.first_token_ms = result.first_token_ms;
    state.duration_ms = Some(result.duration_ms);
    state.finished_at_ms = finished_at_ms;
    if result.success {
        state.failures = 0;
        state.successes = state.successes.saturating_add(1);
        if previous.health != Health::Error
            || state.successes >= u32::from(connection.recovery_threshold)
        {
            state.health = if is_slow(
                result,
                u64::from(connection.slow_first_token_seconds) * 1000,
                u64::from(connection.slow_total_seconds) * 1000,
            ) {
                Health::Warning
            } else {
                Health::Healthy
            };
        }
    } else {
        state.successes = 0;
        state.failures = state.failures.saturating_add(1);
        if state.failures >= u32::from(connection.failure_threshold) {
            state.health = Health::Error;
        }
    }
    state
}
pub(super) fn is_slow(
    result: &CheckResult,
    first_threshold_ms: u64,
    total_threshold_ms: u64,
) -> bool {
    result.duration_ms >= total_threshold_ms
        || result
            .first_token_ms
            .is_some_and(|ms| ms >= first_threshold_ms)
}
pub(crate) fn due(last_ms: u64, interval_seconds: u32, now_ms: u64) -> bool {
    now_ms >= next_due_at(last_ms, interval_seconds)
}
pub(crate) fn next_due_at(last_ms: u64, interval_seconds: u32) -> u64 {
    last_ms.saturating_add(u64::from(interval_seconds) * 1000)
}
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as u64
}
