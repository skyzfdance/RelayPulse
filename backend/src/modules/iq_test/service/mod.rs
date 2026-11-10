use super::actions as OperationAction;
mod history;
mod participants;
mod precheck;
mod questions;
mod runs;

use super::{evidence::Evidence, sqlite::Store, tasks::BlockingActivity, *};
use crate::{
    modules::{
        operation_logs::*,
        sites::{IqConnectionSnapshot, IqCredentialAccess, IqModelSource},
    },
    shared::{
        model_client::{Cancellation, ModelClient},
        persistence::Database,
    },
};
use futures_util::FutureExt;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::watch, task::JoinSet};

pub trait CompletionObserver: Send + Sync {
    fn completed(&self, run: &IqRun);
}
/// 只读的展示元数据，由 application 连接；不触发正式探测或参与 IQ 可用性判定。
pub trait CandidateHealth: Send + Sync {
    fn read(&self, model_ids: &[String]) -> Result<HashMap<String, String>>;
}

#[derive(Clone)]
pub(super) struct ParticipantExecution {
    pub dto: IqParticipant,
    pub connection: IqConnectionSnapshot,
    pub parameters: Map<String, Value>,
    pub sanitize: Sanitizer,
}
pub(super) struct RunControl {
    pub id: String,
    pub retry_max: u32,
    pub cancellation: Cancellation,
    pub done: watch::Sender<bool>,
    pub started: Instant,
    pub user_abort: std::sync::atomic::AtomicBool,
}
struct PrecheckJob {
    cancellation: Cancellation,
    result: watch::Receiver<Option<IqPrecheck>>,
}
#[derive(Default)]
pub(super) struct State {
    revision: u64,
    pub(super) participants: Vec<ParticipantExecution>,
    pub active: Option<IqRun>,
    pub control: Option<Arc<RunControl>>,
    prechecks: HashMap<String, PrecheckJob>,
    closing: bool,
    pub unavailable: bool,
}
pub struct IqTest {
    pub(super) store: Store,
    pub(super) evidence: Evidence,
    source: IqCredentialAccess,
    health: Arc<dyn CandidateHealth>,
    pub(super) client: Arc<ModelClient>,
    pub(super) state: Mutex<State>,
    changes: watch::Sender<u64>,
    jobs: Mutex<Option<JoinSet<()>>>,
    activity: Arc<BlockingActivity>,
    completion: OnceLock<Arc<dyn CompletionObserver>>,
    operation_logs: OperationRecorder,
}
impl IqTest {
    pub fn new(
        source: IqCredentialAccess,
        client: Arc<ModelClient>,
        database: Arc<Database>,
        root: PathBuf,
        health: Arc<dyn CandidateHealth>,
    ) -> Self {
        Self {
            source,
            health,
            client,
            store: Store::new(database),
            evidence: Evidence::new(root),
            state: Mutex::new(State::default()),
            changes: watch::channel(0).0,
            jobs: Mutex::new(Some(JoinSet::new())),
            activity: Arc::default(),
            completion: OnceLock::new(),
            operation_logs: OperationRecorder::default(),
        }
    }
    pub fn with_operation_logs(mut self, logs: Arc<OperationLogs>) -> Self {
        self.operation_logs = OperationRecorder::new(logs);
        self
    }
    pub fn set_completion_observer(
        &self,
        observer: Arc<dyn CompletionObserver>,
    ) -> std::result::Result<(), Arc<dyn CompletionObserver>> {
        self.completion.set(observer)
    }
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
    pub(super) fn changed(&self, state: &mut State) {
        state.revision += 1;
        self.changes.send_replace(state.revision);
    }
    /// 命令开放前完成删除对账及异常运行收敛。没有自动清理仍保留历史的路径。
    pub fn check_ready(&self) -> Result<()> {
        self.evidence.validate_formats()?;
        self.evidence.prepare()?;
        for id in self.evidence.trash_ids()? {
            self.evidence
                .reconcile(&id, self.store.contains_run(&id)?)?;
        }
        let recovered = self.store.recover()?;
        // 上次可能在 DB 收敛后、meta 更新前退出；终态历史也要补齐 running meta。
        for mut run in self.store.runs()? {
            if self.evidence.recover_run(&mut run)? {
                self.store.save_run(&run)?;
            }
        }
        for run in &recovered {
            self.record_system_abort(run);
        }
        self.store.questions()?;
        Ok(())
    }
    pub(super) fn ready(state: &State) -> Result<()> {
        if state.closing {
            Err(IqError::Closing)
        } else if state.unavailable {
            Err(IqError::Storage)
        } else {
            Ok(())
        }
    }
    fn idle(state: &State) -> Result<()> {
        Self::ready(state)?;
        if state.active.is_some() {
            Err(IqError::AlreadyRunning)
        } else {
            Ok(())
        }
    }
    pub(super) async fn blocking<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&Arc<Self>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let held = self.activity.enter();
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let _held = held;
            work(&this)
        })
        .await
        .map_err(|_| IqError::Worker)?
    }
    fn spawn(
        &self,
        work: impl Future<Output = ()> + Send + 'static,
        runtime: &Handle,
    ) -> Result<()> {
        let mut jobs = self.jobs.lock().map_err(|_| IqError::Worker)?;
        let jobs = jobs.as_mut().ok_or(IqError::Closing)?;
        while jobs.try_join_next().is_some() {}
        jobs.spawn_on(work, runtime);
        Ok(())
    }
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closing = true;
        if let Some(control) = &state.control {
            control.cancellation.cancel();
        }
        for job in state.prechecks.values() {
            job.cancellation.cancel();
        }
    }
    pub async fn stop(&self) {
        self.close();
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut jobs) = jobs {
            while jobs.join_next().await.is_some() {}
        }
        self.activity.drained().await;
        self.evidence.drained().await;
    }
    pub(super) fn audit<T>(
        &self,
        action: relaypulse_operations::ActionDefinition,
        target: &str,
        result: &Result<T>,
    ) {
        let (outcome, content) = match result {
            Ok(_) => (
                OperationResult::Success,
                OperationLogContent::Snapshot {
                    fields: vec![SnapshotField {
                        label: "对象".into(),
                        value: target.into(),
                        sensitive: None,
                    }],
                },
            ),
            Err(error) => (
                match error {
                    IqError::AlreadyRunning | IqError::Validation { .. } | IqError::NotFound => {
                        OperationResult::Blocked
                    }
                    IqError::Closing => OperationResult::Cancelled,
                    _ => OperationResult::Failure,
                },
                OperationLogContent::Failure {
                    activity: action.label().into(),
                    target: "IQ 测试".into(),
                    reason: error.to_string(),
                },
            ),
        };
        self.operation_logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            outcome,
            "IQ 测试",
            if result.is_ok() {
                "IQ 操作已完成"
            } else {
                "IQ 操作未完成"
            },
            content,
        ));
    }
    pub(super) fn notify_completed(&self, run: &IqRun) {
        if let Some(observer) = self.completion.get() {
            observer.completed(run);
        }
    }
    pub(super) fn record_system_abort(&self, run: &IqRun) {
        self.operation_logs.record_best_effort(OperationEvent::new(
            OperationAction::IqRunAborted,
            OperationSource::System,
            OperationResult::Cancelled,
            "IQ 测试",
            "IQ 运行已中止，已有证据已保留",
            OperationLogContent::Snapshot {
                fields: vec![SnapshotField {
                    label: "运行 ID".into(),
                    value: run.id.clone(),
                    sensitive: None,
                }],
            },
        ));
    }
}
pub(super) fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as u64
}
struct Done(watch::Sender<bool>);
impl Drop for Done {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

/// 只用于元信息/DTO 的已知连接秘密脱敏，不改写响应事实文件。
#[derive(Clone)]
pub(super) struct Sanitizer {
    patterns: Vec<zeroize::Zeroizing<String>>,
}
impl Sanitizer {
    fn new(connection: &IqConnectionSnapshot, previous: Option<&Self>) -> Self {
        // 重新建立连接后，旧凭据仍属于已知秘密，不能从原 IQ 参数中重新泄露。
        let mut patterns = previous.map(|s| s.patterns.clone()).unwrap_or_default();
        patterns.push(zeroize::Zeroizing::new(
            connection.target.secret.to_string(),
        ));
        for header in &connection.target.headers {
            let key = header.key.to_ascii_lowercase();
            if [
                "authorization",
                "token",
                "secret",
                "key",
                "cookie",
                "password",
                "credential",
            ]
            .iter()
            .any(|word| key.contains(word))
            {
                patterns.push(zeroize::Zeroizing::new(header.value.clone()));
                if key.contains("authorization") {
                    if let Some((_, credential)) = header.value.split_once(' ') {
                        patterns.push(zeroize::Zeroizing::new(credential.trim().to_owned()));
                    }
                }
            }
        }
        patterns.retain(|s| !s.is_empty());
        patterns.sort_by_key(|s| std::cmp::Reverse(s.len()));
        patterns.dedup();
        Self { patterns }
    }
    fn text(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for pattern in &self.patterns {
            text = text.replace(pattern.as_str(), "[REDACTED]");
        }
        text
    }
    pub(super) fn value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => json!(self.text(s)),
            Value::Array(values) => Value::Array(values.iter().map(|v| self.value(v)).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (self.text(k), self.value(v)))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
}
