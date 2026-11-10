//! Read-only projections of immutable attempts; never changes history or contacts a model.
use super::evidence::response;
use super::*;
use crate::shared::{
    file_output::filesystem,
    model_client::{decode_generation_output, GenerationOutputPart},
};
use serde::{Deserialize, Serialize};
use std::{fs, sync::Arc};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqAttempt {
    pub attempt: u32,
    pub status: EntryStatus,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub first_token_ms: Option<u64>,
    pub total_time_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub error_code: Option<String>,
    pub body_complete: bool,
    pub stream_bytes: u64,
    pub usage: serde_json::Value,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IqEntryOutput {
    pub entry_id: String,
    pub attempt: Option<u32>,
    pub attempts: Vec<IqAttempt>,
    pub parts: Vec<GenerationOutputPart>,
    pub encrypted_reasoning: bool,
    pub parsing_complete: bool,
    pub raw_response: Option<String>,
}

impl IqTest {
    pub async fn entry_output(
        self: &Arc<Self>,
        entry_id: String,
        attempt: Option<u32>,
    ) -> Result<IqEntryOutput> {
        self.blocking(move |this| {
            validation::identifier(&entry_id, "entryId")?;
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let run = this
                .store
                .runs()?
                .into_iter()
                .find(|run| run.entries.iter().any(|entry| entry.id == entry_id))
                .ok_or(IqError::NotFound)?;
            let entry = run
                .entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .ok_or(IqError::NotFound)?;
            if !entry.status.terminal() {
                return Err(invalid("entryId", "调用结束后才能读取回放。"));
            }
            if attempt.is_some_and(|attempt| attempt == 0 || attempt > entry.attempt) {
                return Err(invalid("attempt", "尝试次数不存在。"));
            }
            let mut result = IqEntryOutput {
                entry_id,
                attempt: None,
                attempts: Vec::new(),
                parts: Vec::new(),
                encrypted_reasoning: false,
                parsing_complete: true,
                raw_response: None,
            };
            if entry.attempt == 0 {
                return Ok(result);
            }
            validation::identifier(&run.id, "runId")?;
            // Every path component is derived from validated database identities.
            let mut directory = this.evidence.root().to_owned();
            for component in ["runs", &run.id, "entries", &entry.id] {
                directory.push(component);
                if !filesystem::directory(&directory, false)? {
                    return Err(IqError::Evidence);
                }
            }
            for number in 1..=entry.attempt {
                let path = directory.join(format!("meta.{number}.json"));
                filesystem::regular_file(&path)?.ok_or(IqError::Evidence)?;
                let meta: serde_json::Value =
                    serde_json::from_slice(&fs::read(path)?).map_err(|_| IqError::Evidence)?;
                if !response::supported(&meta)
                    || meta["runId"] != run.id
                    || meta["entryId"] != entry.id
                    || meta["attempt"] != number
                {
                    return Err(IqError::Evidence);
                }
                result
                    .attempts
                    .push(serde_json::from_value(meta).map_err(|_| IqError::Evidence)?);
            }
            let selected = attempt.unwrap_or(entry.attempt);
            let path = this.evidence.raw_attempt_path(entry, Some(selected))?;
            let (body, complete_log) = response::read(&path)?;
            let decoded = decode_generation_output(
                run.participants[entry.participant_index].candidate.protocol,
                &body,
            );
            result.raw_response =
                if decoded.parts.is_empty() && !decoded.encrypted_reasoning && !body.is_empty() {
                    Some(String::from_utf8_lossy(&body).into_owned())
                } else {
                    None
                };
            result.attempt = Some(selected);
            result.parts = decoded.parts;
            result.encrypted_reasoning = decoded.encrypted_reasoning;
            result.parsing_complete = decoded.parsing_complete && complete_log;
            Ok(result)
        })
        .await
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/iq_test/replay.rs"]
mod tests;
