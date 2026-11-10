use super::actions as OperationAction;
use super::{
    evidence::AttemptFacts,
    service::{elapsed, ParticipantExecution, RunControl},
    *,
};
use crate::shared::model_client::{
    ClientError, GenerationFailure, GenerationOptions, GenerationRequest, GenerationResult,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

impl IqTest {
    pub(super) async fn run(
        self: Arc<Self>,
        participants: Vec<ParticipantExecution>,
        control: Arc<RunControl>,
    ) {
        let initial = match self.run_details(control.id.clone()).await {
            Ok(run) => run,
            Err(_) => {
                control.cancellation.cancel();
                return;
            }
        };
        // 题目串行，同题所有参与者并发；单个调用失败不会终止队列。
        for question_index in 0..initial.questions.len() {
            if control.cancellation.is_cancelled() {
                break;
            }
            let mut entries = FuturesUnordered::new();
            for (participant_index, participant) in participants.iter().enumerate() {
                let entry_id = initial
                    .entries
                    .iter()
                    .find(|e| {
                        e.question_index == question_index
                            && e.participant_index == participant_index
                    })
                    .map(|e| e.id.clone());
                let Some(entry_id) = entry_id else {
                    control.cancellation.cancel();
                    break;
                };
                entries.push(self.clone().execute_entry(
                    entry_id,
                    participant.clone(),
                    control.clone(),
                ));
            }
            while let Some(result) = entries.next().await {
                if result.is_err() {
                    control.cancellation.cancel();
                }
            }
        }
    }
    async fn execute_entry(
        self: Arc<Self>,
        entry_id: String,
        participant: ParticipantExecution,
        control: Arc<RunControl>,
    ) -> Result<()> {
        for attempt in 1..=control.retry_max + 1 {
            if control.cancellation.is_cancelled() {
                return Ok(());
            }
            let id = entry_id.clone();
            let cancel = control.clone();
            let run = self
                .update_run(control.clone(), move |run| {
                    if cancel.cancellation.is_cancelled() {
                        return;
                    }
                    if let Some(entry) = run.entries.iter_mut().find(|e| e.id == id) {
                        entry.status = EntryStatus::Running;
                        entry.attempt += 1;
                        entry.started_at_ms = Some(now_ms());
                        entry.finished_at_ms = None;
                        entry.first_token_ms = None;
                        entry.total_time_ms = None;
                        entry.input_tokens = None;
                        entry.output_tokens = None;
                        entry.error_code = None;
                        entry.error_message = None;
                        entry.has_raw_stream = false;
                    }
                })
                .await?;
            let entry = run
                .entries
                .iter()
                .find(|e| e.id == entry_id)
                .cloned()
                .ok_or(IqError::Worker)?;
            if entry.status != EntryStatus::Running || entry.attempt != attempt {
                return Ok(());
            }
            // 已经提交的调用起始态仍建立空证据并记录取消；不再发起 HTTP。
            // 若取消先于本次起始事务，attempt 未递增，已在上方退出。
            let started = Instant::now();
            let capture = self
                .evidence
                .start_attempt(&run, &entry, &participant.sanitize, control.retry_max)
                .await;
            let (result, facts) = if let Ok(mut capture) = capture {
                let request = GenerationRequest {
                    target: participant.connection.target.clone(),
                    model: participant.connection.source.model_name.clone(),
                    endpoint: participant.connection.endpoint.clone(),
                    prompt: run.questions[entry.question_index].content.clone(),
                    parameters: participant.parameters.clone(),
                };
                let result = self
                    .client
                    .generate_once(
                        &request,
                        GenerationOptions {
                            output_timeout: None,
                        },
                        &control.cancellation,
                        &mut capture,
                    )
                    .await;
                let facts = capture.facts().await;
                (result, facts)
            } else {
                (
                    Err(ClientError::Capture),
                    AttemptFacts {
                        duration_ms: Some(elapsed(started)),
                        ..Default::default()
                    },
                )
            };
            let failed_request = request_failed(&result);
            let retry = failed_request && attempt <= control.retry_max;
            let id = entry_id.clone();
            let cancel = control.clone();
            self.update_run(control.clone(), move |run| {
                let Some(entry) = run.entries.iter_mut().find(|e| e.id == id) else {
                    return;
                };
                if entry.status.terminal() {
                    return;
                }
                apply_result(entry, &result, &facts);
                if cancel.cancellation.is_cancelled() {
                    entry.status = EntryStatus::Aborted;
                    entry.error_code = Some("cancelled".into());
                    entry.error_message = Some("调用已中止，已接收的证据已保留。".into());
                } else if retry {
                    entry.status = EntryStatus::Running;
                    entry.finished_at_ms = None;
                    entry.error_code = Some("retrying_request".into());
                    entry.error_message = Some(format!(
                        "请求失败，准备第 {} / {} 次重试。",
                        attempt, cancel.retry_max
                    ));
                } else if failed_request && cancel.retry_max > 0 {
                    if let Some(message) = &mut entry.error_message {
                        message.push_str(&format!(" 已达到重试上限（{} 次）。", cancel.retry_max));
                    }
                }
            })
            .await?;
            if !retry {
                break;
            }
        }
        Ok(())
    }
    async fn update_run(
        self: &Arc<Self>,
        control: Arc<RunControl>,
        update: impl FnOnce(&mut IqRun) + Send + 'static,
    ) -> Result<IqRun> {
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            if state.unavailable {
                return Err(IqError::Storage);
            }
            let mut run = state
                .active
                .as_ref()
                .filter(|run| run.id == control.id)
                .cloned()
                .ok_or(IqError::NotFound)?;
            update(&mut run);
            run.summarize(elapsed(control.started));
            if this.store.save_run(&run).is_err() {
                state.unavailable = true;
                control.cancellation.cancel();
                this.changed(&mut state);
                return Err(IqError::Storage);
            }
            state.active = Some(run.clone());
            this.changed(&mut state);
            Ok(run)
        })
        .await
    }
    pub(super) async fn finish_run(
        self: &Arc<Self>,
        control: Arc<RunControl>,
        panicked: bool,
    ) -> Result<()> {
        self.evidence.drained().await;
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            let mut run = state
                .active
                .as_ref()
                .filter(|r| r.id == control.id)
                .cloned()
                .ok_or(IqError::NotFound)?;
            let aborted = panicked
                || control.cancellation.is_cancelled()
                || run.entries.iter().any(|e| !e.status.terminal());
            let now = now_ms().max(run.started_at_ms);
            for entry in &mut run.entries {
                if !entry.status.terminal() {
                    entry.status = EntryStatus::Aborted;
                    entry.finished_at_ms = Some(now);
                    entry.error_code =
                        Some(if panicked { "interrupted" } else { "cancelled" }.into());
                    entry.error_message = Some("调用已中止，已有证据已保留。".into());
                }
            }
            run.status = if aborted {
                RunStatus::Aborted
            } else {
                RunStatus::Completed
            };
            run.finished_at_ms = Some(now);
            run.summarize(elapsed(control.started));
            let result = this.store.save_run(&run);
            if let Err(error) = result {
                state.unavailable = true;
                this.changed(&mut state);
                if control.user_abort.load(Ordering::SeqCst) {
                    this.audit(
                        OperationAction::IqRunAborted,
                        &run.id,
                        &Err::<(), _>(IqError::Storage),
                    );
                }
                return Err(error);
            }
            state.active = None;
            state.control = None;
            state.participants.clear();
            this.changed(&mut state);
            drop(state);
            if run.status == RunStatus::Completed {
                this.notify_completed(&run);
            } else if control.user_abort.load(Ordering::SeqCst) {
                this.audit(OperationAction::IqRunAborted, &run.id, &Ok(()));
            } else {
                this.operation_aborted_on_exit(&run);
            }
            Ok(())
        })
        .await
    }
    fn operation_aborted_on_exit(&self, run: &IqRun) {
        self.record_system_abort(run);
    }
}
fn request_failed(result: &std::result::Result<GenerationResult, ClientError>) -> bool {
    match result {
        Ok(result) => result
            .failure
            .is_some_and(|failure| failure != GenerationFailure::Cancelled),
        Err(
            ClientError::Network
            | ClientError::Status(_)
            | ClientError::Response
            | ClientError::Timeout,
        ) => true,
        // 参数校验、取消和本地写盘失败不是重新请求能够修复的远端错误。
        Err(_) => false,
    }
}
fn apply_result(
    entry: &mut IqEntry,
    result: &std::result::Result<GenerationResult, ClientError>,
    facts: &AttemptFacts,
) {
    entry.finished_at_ms = Some(now_ms());
    entry.has_raw_stream = facts.has_raw_stream;
    entry.first_token_ms = facts.first_token_ms;
    entry.total_time_ms = facts.duration_ms;
    entry.input_tokens = facts.usage.input_tokens.filter(|n| *n <= i64::MAX as u64);
    entry.output_tokens = facts.usage.output_tokens.filter(|n| *n <= i64::MAX as u64);
    let (status, code, message) = match result {
        Ok(result) => {
            entry.first_token_ms = result.first_token_ms;
            entry.total_time_ms = Some(result.duration_ms);
            entry.input_tokens = result
                .metadata
                .usage
                .input_tokens
                .filter(|n| *n <= i64::MAX as u64);
            entry.output_tokens = result
                .metadata
                .usage
                .output_tokens
                .filter(|n| *n <= i64::MAX as u64);
            match result.failure {
                None => (EntryStatus::Completed, None, None),
                Some(GenerationFailure::Timeout) => (
                    EntryStatus::Timeout,
                    Some("output_timeout"),
                    Some("远端请求超时；已接收的响应已保留。"),
                ),
                Some(GenerationFailure::Cancelled) => (
                    EntryStatus::Aborted,
                    Some("cancelled"),
                    Some("调用已中止。"),
                ),
                Some(GenerationFailure::Http) => (
                    EntryStatus::Error,
                    Some("upstream_http"),
                    Some("上游返回非成功 HTTP 状态；原始响应已保留。"),
                ),
                Some(GenerationFailure::Network) => (
                    EntryStatus::Error,
                    Some("network_error"),
                    Some("无法完成远端网络请求；已接收的响应已保留。"),
                ),
                Some(GenerationFailure::InvalidResponse) => (
                    EntryStatus::Error,
                    Some("invalid_response"),
                    Some("响应格式无效、缺少有效输出或未正常结束；原始响应已保留。"),
                ),
            }
        }
        Err(ClientError::Capture) => (
            EntryStatus::Error,
            Some("evidence_write_failed"),
            Some("证据文件写入失败；已停止本次请求并保留已写部分。"),
        ),
        Err(_) => (
            EntryStatus::Error,
            Some("invalid_parameters"),
            Some("本次请求参数或连接无效。"),
        ),
    };
    entry.status = status;
    entry.error_code = code.map(str::to_owned);
    entry.error_message = message.map(str::to_owned);
    if let Ok(result) = result {
        if result.failure == Some(GenerationFailure::Http) {
            entry.error_message = Some(format!(
                "上游返回 HTTP {}；原始响应已保留。",
                result.status_code.unwrap_or(0)
            ));
        }
    }
}
