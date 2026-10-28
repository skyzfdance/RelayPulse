//! 页面投影；不把存储枚举或内部站点关系混入列表 DTO。
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProbeResult {
    Success,
    Slow,
    Failure,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordSummary {
    pub id: String,
    pub site_name: String,
    pub model_name: String,
    pub probed_at: String,
    pub result: ProbeResult,
    pub result_message: String,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordListItem {
    #[serde(flatten)]
    pub summary: RecordSummary,
    pub total_tokens: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordsPage<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub retained_count: u64,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenStatsSummary {
    pub probe_count: u64,
    pub reported_count: u64,
    pub total_reported_count: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub average_tokens: Option<f64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiteTokenStats {
    pub site_id: String,
    pub site_name: String,
    #[serde(flatten)]
    pub usage: TokenStatsSummary,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenStatsPage {
    #[serde(flatten)]
    pub records: RecordsPage<SiteTokenStats>,
    pub summary: TokenStatsSummary,
}
#[derive(Debug, Serialize)]
pub(crate) struct ProbeRecordOptions {
    pub sites: Vec<RecordSiteOption>,
    pub models: Vec<RecordModelOption>,
}
#[derive(Debug, Serialize)]
pub(crate) struct RecordSiteOption {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordModelOption {
    pub name: String,
    pub site_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RecordHeader {
    pub name: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordRequestSummary {
    pub method: Option<String>,
    pub body_bytes: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordRequestContent {
    #[serde(flatten)]
    pub summary: ProbeRecordRequestSummary,
    pub headers: Option<Vec<RecordHeader>>,
    pub body: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordResponseSummary {
    pub state: &'static str,
    pub kind: Option<&'static str>,
    pub received_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordResponseChunk {
    pub index: u64,
    pub elapsed_ms: Option<u64>,
    pub bytes: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordErrorResponse {
    pub body: Option<String>,
    pub body_complete: Option<bool>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordLogFile {
    pub path: String,
    pub can_open_directory: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordResponseContent {
    #[serde(flatten)]
    pub summary: ProbeRecordResponseSummary,
    pub headers: Option<Vec<RecordHeader>>,
    pub chunk_count: Option<u64>,
    pub chunks: Option<Vec<ProbeRecordResponseChunk>>,
    pub error_response: Option<ProbeRecordErrorResponse>,
    pub log_file: Option<ProbeRecordLogFile>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordRequestInfo {
    pub endpoint: Option<String>,
    pub strategy_label: Option<String>,
    pub http_status: Option<u16>,
    pub request_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordSlowThresholds {
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}
#[derive(Debug, Serialize)]
pub(crate) struct ProbeRecordSlowCheck {
    pub comparison: Option<&'static str>,
    pub triggered: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordSlowEvaluation {
    pub status: &'static str,
    pub first_token: ProbeRecordSlowCheck,
    pub duration: ProbeRecordSlowCheck,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRecordDetail {
    #[serde(flatten)]
    pub summary: RecordSummary,
    pub usage: Option<crate::shared::model_client::TokenUsage>,
    pub usage_source: &'static str,
    pub request_info: ProbeRecordRequestInfo,
    pub slow_thresholds: Option<ProbeRecordSlowThresholds>,
    pub slow_evaluation: Option<ProbeRecordSlowEvaluation>,
    pub request_summary: ProbeRecordRequestSummary,
    pub response_summary: ProbeRecordResponseSummary,
}
#[derive(Debug, Serialize)]
pub(crate) struct OpenProbeRecordLogResult {
    pub message: &'static str,
}
