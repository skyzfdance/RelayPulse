use crate::shared::model_client::Protocol;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IqQuestionInput {
    pub content: String,
    #[serde(deserialize_with = "required_option")]
    pub reference_answer: Option<String>,
    pub enabled: bool,
}
fn required_option<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqQuestion {
    pub id: String,
    pub content: String,
    pub reference_answer: Option<String>,
    pub enabled: bool,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqCandidate {
    pub model_key: String,
    pub model_name: String,
    pub site_id: String,
    pub site_name: String,
    pub protocol: Protocol,
    pub health: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct IqHistoryFilterOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<IqHistoryFilterOption>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IqParticipantInput {
    pub model_key: String,
    pub preset: String,
    pub parameters: Map<String, Value>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrecheckStatus {
    Checking,
    Available,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqPrecheck {
    pub status: PrecheckStatus,
    pub message: String,
    pub checked_at_ms: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IqPrecheckResult {
    pub snapshot_id: String,
    pub precheck: IqPrecheck,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqParticipant {
    #[serde(flatten)]
    pub candidate: IqCandidate,
    pub id: String,
    pub snapshot_id: String,
    pub endpoint: String,
    pub created_at_ms: u64,
    pub preset: String,
    pub parameters: Map<String, Value>,
    pub precheck: IqPrecheck,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatus {
    Pending,
    Running,
    Completed,
    Error,
    Timeout,
    Aborted,
}
impl EntryStatus {
    pub(super) fn terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Running)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqEntry {
    pub id: String,
    pub run_id: String,
    pub question_index: usize,
    pub participant_index: usize,
    pub status: EntryStatus,
    pub attempt: u32,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub first_token_ms: Option<u64>,
    pub total_time_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub has_raw_stream: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Completed,
    Aborted,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallResultValue {
    #[default]
    AllSuccess,
    PartialFailure,
    AllFailure,
    Aborted,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqCallResult {
    pub value: CallResultValue,
    pub success: usize,
    pub failed: usize,
    pub timeout: usize,
    pub aborted: usize,
    pub not_started: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionStatus {
    Pending,
    Running,
    Completed,
    Aborted,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqQuestionProgress {
    pub question_index: usize,
    pub terminal_entries: usize,
    pub total_entries: usize,
    pub status: QuestionStatus,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqRunSummary {
    pub call_result: IqCallResult,
    pub total_entries: usize,
    pub terminal_entries: usize,
    pub successful_entries: usize,
    pub progress_percent: u64,
    pub current_question: usize,
    pub total_questions: usize,
    pub total_participants: usize,
    pub duration_ms: u64,
    pub question_progress: Vec<IqQuestionProgress>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqRun {
    pub id: String,
    pub status: RunStatus,
    pub questions: Vec<IqQuestion>,
    pub participants: Vec<IqParticipant>,
    pub entries: Vec<IqEntry>,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub summary: IqRunSummary,
}
impl IqRun {
    pub(super) fn summarize(&mut self, duration_ms: u64) {
        let mut counts = IqCallResult {
            value: CallResultValue::AllSuccess,
            success: 0,
            failed: 0,
            timeout: 0,
            aborted: 0,
            not_started: 0,
        };
        for entry in &self.entries {
            match entry.status {
                EntryStatus::Completed => counts.success += 1,
                EntryStatus::Error => counts.failed += 1,
                EntryStatus::Timeout => counts.timeout += 1,
                EntryStatus::Aborted if entry.attempt > 0 => counts.aborted += 1,
                EntryStatus::Aborted => counts.not_started += 1,
                _ => {}
            }
        }
        counts.value = if self.status == RunStatus::Aborted {
            CallResultValue::Aborted
        } else if counts.failed + counts.timeout == 0 {
            CallResultValue::AllSuccess
        } else if counts.success > 0 {
            CallResultValue::PartialFailure
        } else {
            CallResultValue::AllFailure
        };
        let terminal = self
            .entries
            .iter()
            .filter(|entry| entry.status.terminal())
            .count();
        let question_progress = (0..self.questions.len())
            .map(|question_index| {
                let entries: Vec<_> = self
                    .entries
                    .iter()
                    .filter(|e| e.question_index == question_index)
                    .collect();
                let terminal_entries = entries.iter().filter(|e| e.status.terminal()).count();
                let status = if entries.iter().any(|e| e.status == EntryStatus::Aborted) {
                    QuestionStatus::Aborted
                } else if terminal_entries == entries.len() {
                    QuestionStatus::Completed
                } else if entries.iter().any(|e| e.attempt > 0) {
                    QuestionStatus::Running
                } else {
                    QuestionStatus::Pending
                };
                IqQuestionProgress {
                    question_index,
                    terminal_entries,
                    total_entries: entries.len(),
                    status,
                }
            })
            .collect();
        self.summary = IqRunSummary {
            total_entries: self.entries.len(),
            terminal_entries: terminal,
            successful_entries: counts.success,
            progress_percent: if self.entries.is_empty() {
                0
            } else {
                (terminal * 100 / self.entries.len()) as u64
            },
            current_question: if self.status == RunStatus::Completed {
                self.questions.len()
            } else {
                self.entries
                    .iter()
                    .filter(|e| e.attempt > 0)
                    .map(|e| e.question_index + 1)
                    .max()
                    .unwrap_or(1)
            },
            total_questions: self.questions.len(),
            total_participants: self.participants.len(),
            duration_ms,
            question_progress,
            call_result: counts,
        };
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IqStartInput {
    pub question_ids: Vec<String>,
    pub retry_max: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct IqQuestionSummary {
    pub total: usize,
    pub enabled: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IqSnapshot {
    pub revision: u64,
    pub questions: Vec<IqQuestion>,
    pub question_summary: IqQuestionSummary,
    pub candidates: Vec<IqCandidate>,
    pub participants: Vec<IqParticipant>,
    pub active_run: Option<IqRun>,
    pub runs: Vec<IqRun>,
}
