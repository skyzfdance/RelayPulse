use super::{
    capture::CapturedRecord,
    dto::*,
    error::{RecordError, Result},
    queries::StoredRecord,
};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

impl StoredRecord {
    fn result(&self) -> Result<ProbeResult> {
        match self.outcome.as_str() {
            "succeeded" => Ok(ProbeResult::Success),
            "slow" => Ok(ProbeResult::Slow),
            "failed" => Ok(ProbeResult::Failure),
            _ => Err(RecordError::Unavailable),
        }
    }
    pub(super) fn failed(&self) -> bool {
        self.outcome == "failed"
    }
    fn summary(&self) -> Result<RecordSummary> {
        let result = self.result()?;
        let message = match result {
            ProbeResult::Success => "请求成功。".into(),
            ProbeResult::Slow => "响应偏慢".into(),
            ProbeResult::Failure => match self.failure.as_deref() {
                Some("http") => self.status_code.map_or_else(
                    || "远端返回非成功 HTTP 状态。".into(),
                    |code| format!("远端返回 HTTP {code}。"),
                ),
                Some("network") => "无法完成远端网络请求。".into(),
                Some("timeout") => "远端请求超时。".into(),
                Some("invalid_response") => "响应无有效输出、未正常结束或格式无效。".into(),
                _ => "探测失败。".into(),
            },
        };
        let probed_at =
            OffsetDateTime::from_unix_timestamp_nanos(i128::from(self.probed_at_ms) * 1_000_000)
                .map_err(|_| RecordError::Unavailable)?
                .format(&Rfc3339)
                .map_err(|_| RecordError::Unavailable)?;
        Ok(RecordSummary {
            id: self.id.clone(),
            site_name: self.site_name.clone().unwrap_or_else(|| "未知站点".into()),
            model_name: self.model_name.clone().unwrap_or_else(|| "未知模型".into()),
            probed_at,
            result,
            result_message: message,
            first_token_ms: self.first_token_ms,
            duration_ms: Some(self.duration_ms),
        })
    }
    pub(super) fn list_item(self) -> Result<ProbeRecordListItem> {
        Ok(ProbeRecordListItem {
            summary: self.summary()?,
            total_tokens: self.usage.total_tokens,
        })
    }
    pub(super) fn request_summary(
        &self,
        log: Option<&CapturedRecord>,
    ) -> ProbeRecordRequestSummary {
        ProbeRecordRequestSummary {
            method: self
                .method
                .clone()
                .or_else(|| log.and_then(|l| l.method.clone())),
            body_bytes: log
                .and_then(|l| l.request_body.as_ref())
                .map(|body| body.len() as u64),
        }
    }
    pub(super) fn response_summary(
        &self,
        log: Option<&CapturedRecord>,
    ) -> Result<ProbeRecordResponseSummary> {
        let received_bytes = self
            .received_bytes
            .or_else(|| log.and_then(|l| l.received_bytes));
        let received = self.status_code.is_some()
            || self.response_kind.is_some()
            || log.is_some_and(|l| l.response_started)
            || received_bytes.is_some_and(|n| n > 0);
        let state = if received {
            "received"
        } else if self.capture_enabled.is_some() && self.received_bytes.is_some()
            || log.is_some_and(|l| l.finished)
        {
            "not_received"
        } else {
            "unknown"
        };
        let raw_kind = self
            .response_kind
            .as_deref()
            .or_else(|| log.and_then(|l| l.response_kind.as_deref()));
        let kind = match raw_kind {
            Some("event_stream") => Some("stream"),
            Some("json") => Some("json"),
            Some("other") => Some("other"),
            None => None,
            _ => return Err(RecordError::Unavailable),
        };
        Ok(ProbeRecordResponseSummary {
            state,
            kind,
            received_bytes,
        })
    }
    pub(super) fn detail(self, log: Option<&CapturedRecord>) -> Result<ProbeRecordDetail> {
        let summary = self.summary()?;
        let request_summary = self.request_summary(log);
        let response_summary = self.response_summary(log)?;
        let has_usage = self.usage.input_tokens.is_some()
            || self.usage.output_tokens.is_some()
            || self.usage.total_tokens.is_some();
        let has_thresholds = self.slow_first_token_ms.is_some() || self.slow_total_ms.is_some();
        let slow_evaluation = has_thresholds.then(|| ProbeRecordSlowEvaluation {
            status: if self.failed() {
                "skipped_failure"
            } else {
                "evaluated"
            },
            first_token: self.slow_check(self.first_token_ms, self.slow_first_token_ms),
            duration: self.slow_check(Some(self.duration_ms), self.slow_total_ms),
        });
        let strategy_label = self.strategy_label();
        Ok(ProbeRecordDetail {
            summary,
            usage: has_usage.then_some(self.usage),
            usage_source: if has_usage { "provider" } else { "unreported" },
            request_info: ProbeRecordRequestInfo {
                endpoint: self.endpoint,
                strategy_label,
                http_status: self.status_code,
                request_id: self.request_id,
            },
            slow_thresholds: has_thresholds.then_some(ProbeRecordSlowThresholds {
                first_token_ms: self.slow_first_token_ms,
                duration_ms: self.slow_total_ms,
            }),
            slow_evaluation,
            request_summary,
            response_summary,
        })
    }
    fn slow_check(&self, actual: Option<u64>, threshold: Option<u64>) -> ProbeRecordSlowCheck {
        let comparison = if self.failed() {
            None
        } else {
            actual
                .zip(threshold)
                .map(|(v, t)| if v >= t { "gte" } else { "lt" })
        };
        ProbeRecordSlowCheck {
            comparison,
            triggered: self.outcome == "slow" && comparison == Some("gte"),
        }
    }
    fn strategy_label(&self) -> Option<String> {
        let policy = self.policy.as_ref()?;
        let source = policy["source"].as_str()?;
        if source == "custom_request" || self.protocol.as_deref() == Some("custom-http") {
            return Some("自定义请求".into());
        }
        let mut effort = None;
        if let Some(parameters) = policy["parameters"].as_array() {
            for parameter in parameters {
                let key = parameter["key"].as_str();
                match (self.protocol.as_deref(), key) {
                    (Some("openai-compatible"), Some("reasoning_effort"))
                    | (Some("openai-responses"), Some("reasoning.effort")) => {
                        effort = parameter["value"].as_str().map(str::to_owned)
                    }
                    (Some("openai-responses"), Some("reasoning")) => {
                        effort = parameter["value"]
                            .as_str()
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .and_then(|v| v["effort"].as_str().map(str::to_owned));
                    }
                    _ => {}
                }
            }
        }
        Some(match effort.as_deref() {
            Some("none") => "不使用推理".into(),
            Some("minimal") => "最低推理".into(),
            Some("low") => "低推理".into(),
            Some("medium") => "中推理".into(),
            Some("high") => "高推理".into(),
            Some("xhigh") => "超高推理".into(),
            Some(value) => format!("推理：{value}"),
            None => match source {
                "model_policy" => "模型独立策略",
                _ => "站点默认策略",
            }
            .into(),
        })
    }
}
pub(super) fn request(
    row: StoredRecord,
    mut log: Option<CapturedRecord>,
) -> ProbeRecordRequestContent {
    let summary = row.request_summary(log.as_ref());
    ProbeRecordRequestContent {
        summary,
        headers: log.as_mut().and_then(|l| l.request_headers.take()),
        body: log.and_then(|l| l.request_body),
    }
}
pub(super) fn response(
    row: StoredRecord,
    mut log: Option<CapturedRecord>,
) -> Result<ProbeRecordResponseContent> {
    let summary = row.response_summary(log.as_ref())?;
    let error_response =
        (row.failed() && summary.state == "received").then(|| ProbeRecordErrorResponse {
            body: log.as_mut().and_then(|l| l.response_body.take()),
            body_complete: row
                .body_complete
                .or_else(|| log.as_ref().and_then(|l| l.body_complete)),
        });
    let chunks = if summary.kind == Some("stream") {
        log.as_mut().and_then(|l| l.chunks.take())
    } else {
        None
    };
    let log_file = log
        .as_ref()
        .map(|l| {
            l.path
                .to_str()
                .map(|path| ProbeRecordLogFile {
                    path: path.into(),
                    can_open_directory: true,
                })
                .ok_or(RecordError::ResponseUnavailable)
        })
        .transpose()?;
    Ok(ProbeRecordResponseContent {
        summary,
        headers: log.and_then(|l| l.response_headers),
        chunk_count: chunks.as_ref().map(|c| c.len() as u64),
        chunks,
        error_response,
        log_file,
    })
}
