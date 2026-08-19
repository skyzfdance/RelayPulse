use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tauri::async_runtime::{self, JoinHandle};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    diagnostics::Diagnostics,
    domain::{Clock, SystemClock, TimestampMs},
    persistence::{DatabaseError, Repository},
};

use super::{parse_rankings, IntelligenceParseError, RawRankingEntry};

pub const ALLOWED_INTERVAL_MINUTES: [u32; 4] = [30, 60, 180, 360];

#[derive(Debug, Clone, PartialEq)]
pub struct IntelligenceRankingEntry {
    pub rank: u32,
    pub model: String,
    pub effort: String,
    pub iq: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntelligenceAutoFetchSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub fetch_on_app_start: bool,
    pub fetch_on_network_restore: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntelligenceLadderRuntimeState {
    Idle,
    Running,
    Failed,
    ScraperUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntelligenceSafeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntelligenceLadderSnapshot {
    pub revision: u64,
    pub entries: Vec<IntelligenceRankingEntry>,
    pub fetched_at: Option<String>,
    pub is_stale: bool,
    pub last_error: Option<IntelligenceSafeError>,
    pub runtime_state: IntelligenceLadderRuntimeState,
    pub last_successful_at: Option<String>,
    pub next_scheduled_at: Option<String>,
    pub settings: IntelligenceAutoFetchSettings,
    pub allowed_interval_minutes: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntelligenceRefreshResult {
    Started,
    AlreadyRunning,
    ScraperUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IntelligenceScrapeError {
    #[error("the network request failed")]
    Network,
    #[error("the page renderer timed out")]
    RenderTimeout,
    #[error("the page renderer is unavailable")]
    RenderUnavailable,
    #[error("the ranking target was not found")]
    TargetNotFound,
    #[error("the scraper worker exited unexpectedly")]
    WorkerCrashed,
    #[error("the scraper worker protocol was invalid")]
    WorkerProtocolInvalid,
    #[error("the rendered ranking snapshot was invalid")]
    InvalidSnapshot,
    #[error("the scraper worker timed out")]
    WorkerTimeout,
    #[error("the scraper worker could not be terminated")]
    WorkerKillFailed,
    #[error("the scraper operation was cancelled")]
    Cancelled,
    #[error("an internal scraper error occurred")]
    Internal,
}

pub trait IntelligenceScraper: Send + Sync {
    fn scrape(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<RawRankingEntry>, IntelligenceScrapeError>> + Send>>;
}

#[derive(Debug, Error)]
pub enum IntelligenceLadderError {
    #[error("intelligence ladder settings are invalid")]
    InvalidSettings { field: &'static str },
    #[error("the intelligence ladder database is unavailable")]
    DatabaseUnavailable,
}

impl From<DatabaseError> for IntelligenceLadderError {
    fn from(_: DatabaseError) -> Self {
        Self::DatabaseUnavailable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Idle,
    Running,
    Failed,
    CircuitOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshTrigger {
    Manual,
    Scheduled,
    ApplicationStart,
    NetworkRestore,
}

type ChangeCallback = dyn Fn(u64, Option<&str>) + Send + Sync;

impl RefreshTrigger {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
            Self::ApplicationStart => "application_start",
            Self::NetworkRestore => "network_restore",
        }
    }
}

struct RuntimeState {
    session: SessionState,
    next_scheduled_at: Option<TimestampMs>,
}

struct ServiceInner {
    repository: Repository,
    scraper: Arc<dyn IntelligenceScraper>,
    clock: Arc<dyn Clock>,
    runtime: Mutex<RuntimeState>,
    on_change: Arc<ChangeCallback>,
    diagnostics: Option<Diagnostics>,
}

#[derive(Clone)]
pub struct IntelligenceLadderService {
    inner: Arc<ServiceInner>,
}

impl IntelligenceLadderService {
    pub fn new(repository: Repository, scraper: Arc<dyn IntelligenceScraper>) -> Self {
        Self::with_dependencies(repository, scraper, Arc::new(SystemClock), |_, _| {})
    }

    pub fn with_dependencies<F>(
        repository: Repository,
        scraper: Arc<dyn IntelligenceScraper>,
        clock: Arc<dyn Clock>,
        on_change: F,
    ) -> Self
    where
        F: Fn(u64, Option<&str>) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(ServiceInner {
                repository,
                scraper,
                clock,
                runtime: Mutex::new(RuntimeState {
                    session: SessionState::Idle,
                    next_scheduled_at: None,
                }),
                on_change: Arc::new(on_change),
                diagnostics: None,
            }),
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Option<Diagnostics>) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("diagnostics are configured before sharing the service")
            .diagnostics = diagnostics;
        self
    }

    pub async fn snapshot(&self) -> Result<IntelligenceLadderSnapshot, IntelligenceLadderError> {
        let persisted = self.inner.repository.intelligence_ladder_state().await?;
        let runtime = self
            .inner
            .runtime
            .lock()
            .map_err(|_| IntelligenceLadderError::DatabaseUnavailable)?;
        let state = match runtime.session {
            SessionState::Idle if persisted.last_error.is_some() => {
                IntelligenceLadderRuntimeState::Failed
            }
            SessionState::Idle => IntelligenceLadderRuntimeState::Idle,
            SessionState::Running => IntelligenceLadderRuntimeState::Running,
            SessionState::Failed => IntelligenceLadderRuntimeState::Failed,
            SessionState::CircuitOpen => IntelligenceLadderRuntimeState::ScraperUnavailable,
        };
        let fetched_at = persisted.fetched_at.and_then(format_timestamp);
        Ok(IntelligenceLadderSnapshot {
            revision: persisted.revision,
            entries: persisted.entries,
            fetched_at: fetched_at.clone(),
            is_stale: persisted.last_error.is_some() && fetched_at.is_some(),
            last_error: persisted.last_error,
            runtime_state: state,
            last_successful_at: fetched_at,
            next_scheduled_at: runtime.next_scheduled_at.and_then(format_timestamp),
            settings: persisted.settings,
            allowed_interval_minutes: ALLOWED_INTERVAL_MINUTES.to_vec(),
        })
    }

    pub async fn refresh(&self) -> Result<IntelligenceRefreshResult, IntelligenceLadderError> {
        self.refresh_with_operation(None).await
    }

    pub async fn refresh_with_operation(
        &self,
        operation_id: Option<String>,
    ) -> Result<IntelligenceRefreshResult, IntelligenceLadderError> {
        self.refresh_with_trigger(RefreshTrigger::Manual, operation_id)
            .await
    }

    async fn refresh_with_trigger(
        &self,
        trigger: RefreshTrigger,
        operation_id: Option<String>,
    ) -> Result<IntelligenceRefreshResult, IntelligenceLadderError> {
        let acquisition = {
            let mut runtime = self
                .inner
                .runtime
                .lock()
                .map_err(|_| IntelligenceLadderError::DatabaseUnavailable)?;
            match runtime.session {
                SessionState::Running => IntelligenceRefreshResult::AlreadyRunning,
                SessionState::CircuitOpen => IntelligenceRefreshResult::ScraperUnavailable,
                SessionState::Idle | SessionState::Failed => {
                    runtime.session = SessionState::Running;
                    IntelligenceRefreshResult::Started
                }
            }
        };
        if acquisition != IntelligenceRefreshResult::Started {
            if let Some(diagnostics) = &self.inner.diagnostics {
                diagnostics.record_intelligence_ladder_refresh_skipped(
                    operation_id.as_deref(),
                    trigger.as_str(),
                    match acquisition {
                        IntelligenceRefreshResult::AlreadyRunning => "already_running",
                        IntelligenceRefreshResult::ScraperUnavailable => "circuit_open",
                        IntelligenceRefreshResult::Started => unreachable!(),
                    },
                );
            }
            return Ok(acquisition);
        }

        let revision = match self
            .inner
            .repository
            .touch_intelligence_ladder_revision()
            .await
        {
            Ok(revision) => revision,
            Err(error) => {
                self.set_session(SessionState::Failed);
                if let Some(diagnostics) = &self.inner.diagnostics {
                    diagnostics
                        .record_background_error("intelligence-ladder-refresh-start", "database");
                }
                return Err(error.into());
            }
        };
        let refresh_id = format!("iq_{}", Uuid::new_v4().simple());
        if let Some(diagnostics) = &self.inner.diagnostics {
            diagnostics.record_intelligence_ladder_refresh_started(
                operation_id.as_deref(),
                &refresh_id,
                trigger.as_str(),
                revision,
            );
        }
        (self.inner.on_change)(revision, operation_id.as_deref());
        let service = self.clone();
        async_runtime::spawn(async move {
            service.run_refresh(refresh_id, trigger, operation_id).await;
        });
        Ok(IntelligenceRefreshResult::Started)
    }

    pub async fn update_settings(
        &self,
        settings: IntelligenceAutoFetchSettings,
    ) -> Result<IntelligenceLadderSnapshot, IntelligenceLadderError> {
        self.update_settings_with_operation(settings, None).await
    }

    pub async fn update_settings_with_operation(
        &self,
        settings: IntelligenceAutoFetchSettings,
        operation_id: Option<String>,
    ) -> Result<IntelligenceLadderSnapshot, IntelligenceLadderError> {
        validate_settings(&settings)?;
        let revision = self
            .inner
            .repository
            .update_intelligence_ladder_settings(settings.clone(), self.inner.clock.now())
            .await?;
        if let Some(diagnostics) = &self.inner.diagnostics {
            diagnostics.record_intelligence_ladder_settings_updated(
                operation_id.as_deref(),
                revision,
                settings.enabled,
                settings.interval_minutes,
                settings.fetch_on_app_start,
                settings.fetch_on_network_restore,
            );
        }
        {
            let mut runtime = self
                .inner
                .runtime
                .lock()
                .map_err(|_| IntelligenceLadderError::DatabaseUnavailable)?;
            runtime.next_scheduled_at = next_scheduled_at(self.inner.clock.now(), &settings);
        }
        (self.inner.on_change)(revision, operation_id.as_deref());
        self.snapshot().await
    }

    pub async fn try_automatic_refresh(&self) -> Result<(), IntelligenceLadderError> {
        let settings = self.snapshot().await?.settings;
        if settings.enabled {
            let _ = self
                .refresh_with_trigger(RefreshTrigger::Scheduled, None)
                .await?;
        }
        Ok(())
    }

    pub async fn network_restored(&self) -> Result<(), IntelligenceLadderError> {
        let settings = self.snapshot().await?.settings;
        if !settings.enabled || !settings.fetch_on_network_restore {
            if let Some(diagnostics) = &self.inner.diagnostics {
                diagnostics.record_intelligence_ladder_refresh_skipped(
                    None,
                    RefreshTrigger::NetworkRestore.as_str(),
                    "trigger_disabled",
                );
            }
        } else {
            let _ = self
                .refresh_with_trigger(RefreshTrigger::NetworkRestore, None)
                .await?;
        }
        Ok(())
    }

    pub async fn reconcile_schedule(
        &self,
    ) -> Result<IntelligenceAutoFetchSettings, IntelligenceLadderError> {
        let settings = self.snapshot().await?.settings;
        let mut runtime = self
            .inner
            .runtime
            .lock()
            .map_err(|_| IntelligenceLadderError::DatabaseUnavailable)?;
        runtime.next_scheduled_at = next_scheduled_at(self.inner.clock.now(), &settings);
        Ok(settings)
    }

    async fn run_refresh(
        &self,
        refresh_id: String,
        trigger: RefreshTrigger,
        operation_id: Option<String>,
    ) {
        let started_at = Instant::now();
        let result = self.inner.scraper.scrape().await.and_then(|raw| {
            parse_rankings(raw).map_err(|error| match error {
                IntelligenceParseError::InvalidEntryCount
                | IntelligenceParseError::InvalidText
                | IntelligenceParseError::InvalidIq
                | IntelligenceParseError::DuplicateIdentity => {
                    IntelligenceScrapeError::InvalidSnapshot
                }
            })
        });
        match result {
            Ok(entries) => {
                let entry_count = u64::try_from(entries.len()).unwrap_or(u64::MAX);
                if let Ok(revision) = self
                    .inner
                    .repository
                    .replace_intelligence_ladder_snapshot(entries, self.inner.clock.now())
                    .await
                {
                    self.set_session(SessionState::Idle);
                    if let Some(diagnostics) = &self.inner.diagnostics {
                        diagnostics.record_intelligence_ladder_refresh_finished(
                            operation_id.as_deref(),
                            &refresh_id,
                            trigger.as_str(),
                            elapsed_millis(started_at),
                            "succeeded",
                            None,
                            Some(entry_count),
                            Some(revision),
                            false,
                        );
                    }
                    (self.inner.on_change)(revision, operation_id.as_deref());
                } else {
                    self.finish_with_error(
                        IntelligenceScrapeError::Internal,
                        "database",
                        &refresh_id,
                        trigger,
                        started_at,
                        operation_id.as_deref(),
                    )
                    .await;
                }
            }
            Err(error) => {
                let error_code = scrape_error_code(&error);
                self.finish_with_error(
                    error,
                    error_code,
                    &refresh_id,
                    trigger,
                    started_at,
                    operation_id.as_deref(),
                )
                .await;
            }
        }
    }

    async fn finish_with_error(
        &self,
        error: IntelligenceScrapeError,
        error_code: &'static str,
        refresh_id: &str,
        trigger: RefreshTrigger,
        started_at: Instant,
        operation_id: Option<&str>,
    ) {
        let circuit_open = error == IntelligenceScrapeError::WorkerKillFailed;
        let safe_error = safe_error(error);
        let revision = self
            .inner
            .repository
            .record_intelligence_ladder_error(safe_error, self.inner.clock.now())
            .await;
        self.set_session(if circuit_open {
            SessionState::CircuitOpen
        } else {
            SessionState::Failed
        });
        if let Ok(revision) = revision {
            if let Some(diagnostics) = &self.inner.diagnostics {
                diagnostics.record_intelligence_ladder_refresh_finished(
                    operation_id,
                    refresh_id,
                    trigger.as_str(),
                    elapsed_millis(started_at),
                    "failed",
                    Some(error_code),
                    None,
                    Some(revision),
                    circuit_open,
                );
            }
            (self.inner.on_change)(revision, operation_id);
        } else if let Some(diagnostics) = &self.inner.diagnostics {
            diagnostics.record_intelligence_ladder_refresh_finished(
                operation_id,
                refresh_id,
                trigger.as_str(),
                elapsed_millis(started_at),
                "failed",
                Some("database"),
                None,
                None,
                circuit_open,
            );
        }
    }

    fn set_session(&self, session: SessionState) {
        match self.inner.runtime.lock() {
            Ok(mut runtime) => runtime.session = session,
            Err(_) => self.record_scheduler_error(
                "intelligence-ladder-runtime-state",
                "runtime_state_lock_failed",
            ),
        }
    }

    fn record_scheduler_error(&self, operation: &'static str, error_code: &'static str) {
        if let Some(diagnostics) = &self.inner.diagnostics {
            diagnostics.record_background_error(operation, error_code);
        }
    }
}

pub struct IntelligenceLadderSchedulerHandle {
    shutdown: watch::Sender<bool>,
    runner: Option<JoinHandle<()>>,
    diagnostics: Option<Diagnostics>,
}

impl IntelligenceLadderSchedulerHandle {
    pub async fn shutdown(mut self) -> bool {
        let _ = self.shutdown.send(true);
        let Some(mut runner) = self.runner.take() else {
            return true;
        };
        match tokio::time::timeout(Duration::from_secs(5), &mut runner).await {
            Ok(Ok(())) => true,
            Ok(Err(_)) => {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_background_error(
                        "intelligence-ladder-scheduler-shutdown",
                        "scheduler_runner_failed",
                    );
                }
                false
            }
            Err(_) => {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_background_error(
                        "intelligence-ladder-scheduler-shutdown",
                        "scheduler_shutdown_timeout",
                    );
                }
                runner.abort();
                let _ = runner.await;
                false
            }
        }
    }
}

impl Drop for IntelligenceLadderSchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(runner) = self.runner.take() {
            runner.abort();
        }
    }
}

pub fn start_intelligence_ladder_scheduler(
    service: IntelligenceLadderService,
) -> IntelligenceLadderSchedulerHandle {
    let (shutdown, receiver) = watch::channel(false);
    let diagnostics = service.inner.diagnostics.clone();
    let runner = async_runtime::spawn(run_scheduler(service, receiver));
    IntelligenceLadderSchedulerHandle {
        shutdown,
        runner: Some(runner),
        diagnostics,
    }
}

async fn run_scheduler(service: IntelligenceLadderService, mut shutdown: watch::Receiver<bool>) {
    let settings = match service.reconcile_schedule().await {
        Ok(settings) => settings,
        Err(_) => {
            service.record_scheduler_error(
                "intelligence-ladder-scheduler-start",
                "schedule_reconcile_failed",
            );
            return;
        }
    };
    if settings.enabled
        && settings.fetch_on_app_start
        && service
            .refresh_with_trigger(RefreshTrigger::ApplicationStart, None)
            .await
            .is_err()
    {
        service.record_scheduler_error(
            "intelligence-ladder-application-start-refresh",
            "refresh_start_failed",
        );
    }
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }
            _ = interval.tick() => {
                let snapshot = match service.snapshot().await {
                    Ok(snapshot) => snapshot,
                    Err(_) => {
                        service.record_scheduler_error(
                            "intelligence-ladder-scheduler-snapshot",
                            "snapshot_unavailable",
                        );
                        continue;
                    },
                };
                if !snapshot.settings.enabled { continue; }
                let due = snapshot
                    .next_scheduled_at
                    .as_deref()
                    .and_then(parse_timestamp)
                    .is_some_and(|scheduled| scheduled <= service.inner.clock.now());
                if due {
                    if service.reconcile_schedule().await.is_err() {
                        service.record_scheduler_error(
                            "intelligence-ladder-scheduler-reconcile",
                            "schedule_reconcile_failed",
                        );
                    }
                    if service
                        .refresh_with_trigger(RefreshTrigger::Scheduled, None)
                        .await
                        .is_err()
                    {
                        service.record_scheduler_error(
                            "intelligence-ladder-scheduled-refresh",
                            "refresh_start_failed",
                        );
                    }
                }
            }
        }
    }
}

fn validate_settings(
    settings: &IntelligenceAutoFetchSettings,
) -> Result<(), IntelligenceLadderError> {
    if !ALLOWED_INTERVAL_MINUTES.contains(&settings.interval_minutes) {
        return Err(IntelligenceLadderError::InvalidSettings {
            field: "interval_minutes",
        });
    }
    Ok(())
}

fn next_scheduled_at(
    now: TimestampMs,
    settings: &IntelligenceAutoFetchSettings,
) -> Option<TimestampMs> {
    if !settings.enabled {
        return None;
    }
    let delta = i64::from(settings.interval_minutes).checked_mul(60_000)?;
    TimestampMs::new(now.as_i64().checked_add(delta)?).ok()
}

fn format_timestamp(value: TimestampMs) -> Option<String> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(value.as_i64()) * 1_000_000)
        .ok()
        .and_then(|value| value.format(&Rfc3339).ok())
}

fn parse_timestamp(value: &str) -> Option<TimestampMs> {
    let value = OffsetDateTime::parse(value, &Rfc3339).ok()?;
    let milliseconds = value.unix_timestamp_nanos().checked_div(1_000_000)?;
    TimestampMs::new(i64::try_from(milliseconds).ok()?).ok()
}

fn safe_error(error: IntelligenceScrapeError) -> IntelligenceSafeError {
    let (code, message) = match error {
        IntelligenceScrapeError::Network => ("network", "最近一次获取失败，当前显示上次成功数据。"),
        IntelligenceScrapeError::RenderTimeout => {
            ("render_timeout", "页面渲染超时，已保留上次成功数据。")
        }
        IntelligenceScrapeError::RenderUnavailable => (
            "render_unavailable",
            "页面渲染服务暂不可用，已保留上次成功数据。",
        ),
        IntelligenceScrapeError::TargetNotFound => {
            ("target_not_found", "未找到完整排名，已保留上次成功数据。")
        }
        IntelligenceScrapeError::WorkerCrashed => {
            ("worker_crashed", "采集进程异常退出，已保留上次成功数据。")
        }
        IntelligenceScrapeError::WorkerProtocolInvalid => (
            "worker_protocol_invalid",
            "采集协议无效，已保留上次成功数据。",
        ),
        IntelligenceScrapeError::InvalidSnapshot => (
            "invalid_snapshot",
            "排名页面结构不兼容，已保留上次成功数据。",
        ),
        IntelligenceScrapeError::WorkerTimeout => {
            ("worker_timeout", "采集任务超时，已保留上次成功数据。")
        }
        IntelligenceScrapeError::WorkerKillFailed => {
            ("worker_kill_failed", "采集进程无法安全回收，服务已停止。")
        }
        IntelligenceScrapeError::Cancelled => ("cancelled", "本次采集已取消，已保留上次成功数据。"),
        IntelligenceScrapeError::Internal => {
            ("internal", "智力天梯暂时无法更新，已保留上次成功数据。")
        }
    };
    IntelligenceSafeError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn scrape_error_code(error: &IntelligenceScrapeError) -> &'static str {
    match error {
        IntelligenceScrapeError::Network => "network",
        IntelligenceScrapeError::RenderTimeout => "render_timeout",
        IntelligenceScrapeError::RenderUnavailable => "render_unavailable",
        IntelligenceScrapeError::TargetNotFound => "target_not_found",
        IntelligenceScrapeError::WorkerCrashed => "worker_crashed",
        IntelligenceScrapeError::WorkerProtocolInvalid => "worker_protocol_invalid",
        IntelligenceScrapeError::InvalidSnapshot => "invalid_snapshot",
        IntelligenceScrapeError::WorkerTimeout => "worker_timeout",
        IntelligenceScrapeError::WorkerKillFailed => "worker_kill_failed",
        IntelligenceScrapeError::Cancelled => "cancelled",
        IntelligenceScrapeError::Internal => "internal",
    }
}

fn elapsed_millis(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::Notify;

    use crate::{domain::FixedClock, persistence::Database};

    use super::*;

    struct BlockingScraper {
        calls: AtomicUsize,
        release: Arc<Notify>,
        result: Mutex<Option<Result<Vec<RawRankingEntry>, IntelligenceScrapeError>>>,
    }

    impl BlockingScraper {
        fn success() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                release: Arc::new(Notify::new()),
                result: Mutex::new(Some(Ok(vec![
                    RawRankingEntry {
                        model: "C".into(),
                        effort: "high".into(),
                        iq: 80.0,
                    },
                    RawRankingEntry {
                        model: "A".into(),
                        effort: "high".into(),
                        iq: 100.0,
                    },
                    RawRankingEntry {
                        model: "B".into(),
                        effort: "high".into(),
                        iq: 90.0,
                    },
                ]))),
            })
        }

        fn failure(error: IntelligenceScrapeError) -> Arc<Self> {
            let value = Self::success();
            *value.result.lock().expect("result lock") = Some(Err(error));
            value
        }
    }

    impl IntelligenceScraper for BlockingScraper {
        fn scrape(
            &self,
        ) -> Pin<
            Box<dyn Future<Output = Result<Vec<RawRankingEntry>, IntelligenceScrapeError>> + Send>,
        > {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let release = self.release.clone();
            let result = self
                .result
                .lock()
                .expect("result lock")
                .take()
                .expect("single scrape result");
            Box::pin(async move {
                release.notified().await;
                result
            })
        }
    }

    fn service(scraper: Arc<dyn IntelligenceScraper>) -> IntelligenceLadderService {
        IntelligenceLadderService::with_dependencies(
            Repository::new(Database::open_in_memory().expect("database opens")),
            scraper,
            Arc::new(FixedClock::new(
                TimestampMs::new(1_700_000_000_000).expect("timestamp is valid"),
            )),
            |_, _| {},
        )
    }

    fn service_with_diagnostics(
        scraper: Arc<dyn IntelligenceScraper>,
        diagnostics: Diagnostics,
    ) -> IntelligenceLadderService {
        service(scraper).with_diagnostics(Some(diagnostics))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_is_non_waiting_single_flight_and_commits_a_complete_snapshot() {
        let scraper = BlockingScraper::success();
        let service = service(scraper.clone());

        assert_eq!(
            service.refresh().await.expect("refresh starts"),
            IntelligenceRefreshResult::Started
        );
        assert_eq!(
            service.refresh().await.expect("busy is reported"),
            IntelligenceRefreshResult::AlreadyRunning
        );
        for _ in 0..20 {
            if scraper.calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(scraper.calls.load(Ordering::SeqCst), 1);
        scraper.release.notify_one();
        tokio::task::yield_now().await;
        for _ in 0..20 {
            if service
                .snapshot()
                .await
                .expect("snapshot loads")
                .runtime_state
                == IntelligenceLadderRuntimeState::Idle
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        let snapshot = service.snapshot().await.expect("snapshot loads");
        assert_eq!(snapshot.entries.len(), 3);
        assert_eq!(snapshot.entries[0].model, "A");
        assert_eq!(snapshot.runtime_state, IntelligenceLadderRuntimeState::Idle);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn kill_failure_opens_the_session_circuit_without_a_second_worker() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        let scraper = BlockingScraper::failure(IntelligenceScrapeError::WorkerKillFailed);
        let service = service_with_diagnostics(scraper.clone(), diagnostics.clone());
        assert_eq!(
            service.refresh().await.expect("refresh starts"),
            IntelligenceRefreshResult::Started
        );
        scraper.release.notify_one();
        for _ in 0..20 {
            if service
                .snapshot()
                .await
                .expect("snapshot loads")
                .runtime_state
                == IntelligenceLadderRuntimeState::ScraperUnavailable
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            service.refresh().await.expect("circuit is reported"),
            IntelligenceRefreshResult::ScraperUnavailable
        );
        assert_eq!(scraper.calls.load(Ordering::SeqCst), 1);
        let log = fs::read_to_string(diagnostics.log_directory().join("relaypulse.log"))
            .expect("diagnostic log reads");
        assert!(log.contains(r#""errorCode":"worker_kill_failed""#));
        assert!(log.contains(r#""circuitOpen":true"#));
        assert!(log.contains(r#""reason":"circuit_open""#));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_are_validated_and_disabled_values_are_preserved() {
        let service = service(BlockingScraper::success());
        let settings = IntelligenceAutoFetchSettings {
            enabled: false,
            interval_minutes: 180,
            fetch_on_app_start: true,
            fetch_on_network_restore: true,
        };
        let snapshot = service
            .update_settings(settings.clone())
            .await
            .expect("settings save");
        assert_eq!(snapshot.settings, settings);
        assert!(snapshot.next_scheduled_at.is_none());
        assert!(matches!(
            service
                .update_settings(IntelligenceAutoFetchSettings {
                    interval_minutes: 10,
                    ..snapshot.settings
                })
                .await,
            Err(IntelligenceLadderError::InvalidSettings {
                field: "interval_minutes"
            })
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn automatic_triggers_respect_settings_and_never_queue_a_second_worker() {
        let scraper = BlockingScraper::success();
        let service = service(scraper.clone());

        service
            .try_automatic_refresh()
            .await
            .expect("disabled automatic refresh is ignored");
        service
            .network_restored()
            .await
            .expect("disabled network refresh is ignored");
        assert_eq!(scraper.calls.load(Ordering::SeqCst), 0);

        service
            .update_settings(IntelligenceAutoFetchSettings {
                enabled: true,
                interval_minutes: 60,
                fetch_on_app_start: false,
                fetch_on_network_restore: false,
            })
            .await
            .expect("settings save");
        service
            .network_restored()
            .await
            .expect("network switch is respected");
        assert_eq!(scraper.calls.load(Ordering::SeqCst), 0);

        service
            .try_automatic_refresh()
            .await
            .expect("scheduled refresh starts");
        service
            .try_automatic_refresh()
            .await
            .expect("busy scheduled refresh is skipped");
        for _ in 0..20 {
            if scraper.calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(scraper.calls.load(Ordering::SeqCst), 1);
        scraper.release.notify_one();
    }

    #[test]
    fn schedule_is_rebuilt_from_now_and_disabled_settings_have_no_plan() {
        let now = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        let enabled = IntelligenceAutoFetchSettings {
            enabled: true,
            interval_minutes: 180,
            fetch_on_app_start: true,
            fetch_on_network_restore: true,
        };

        assert_eq!(
            next_scheduled_at(now, &enabled),
            TimestampMs::new(now.as_i64() + 180 * 60_000).ok()
        );
        assert_eq!(
            next_scheduled_at(
                now,
                &IntelligenceAutoFetchSettings {
                    enabled: false,
                    ..enabled
                }
            ),
            None
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn diagnostics_correlate_refreshes_without_recording_ranking_content() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        let scraper = BlockingScraper::success();
        let service = service_with_diagnostics(scraper.clone(), diagnostics.clone());

        assert_eq!(
            service
                .refresh_with_operation(Some("op_intelligence_refresh".to_owned()))
                .await
                .expect("refresh starts"),
            IntelligenceRefreshResult::Started
        );
        assert_eq!(
            service
                .refresh_with_operation(Some("op_intelligence_busy".to_owned()))
                .await
                .expect("busy is reported"),
            IntelligenceRefreshResult::AlreadyRunning
        );
        scraper.release.notify_one();
        for _ in 0..50 {
            if service
                .snapshot()
                .await
                .expect("snapshot loads")
                .runtime_state
                == IntelligenceLadderRuntimeState::Idle
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        service
            .update_settings_with_operation(
                IntelligenceAutoFetchSettings {
                    enabled: true,
                    interval_minutes: 180,
                    fetch_on_app_start: false,
                    fetch_on_network_restore: true,
                },
                Some("op_intelligence_settings".to_owned()),
            )
            .await
            .expect("settings save");

        let log = fs::read_to_string(diagnostics.log_directory().join("relaypulse.log"))
            .expect("diagnostic log reads");
        assert!(log.contains(r#""event":"intelligence-ladder-refresh-started""#));
        assert!(log.contains(r#""operationId":"op_intelligence_refresh""#));
        assert!(log.contains(r#""trigger":"manual""#));
        assert!(log.contains(r#""event":"intelligence-ladder-refresh-skipped""#));
        assert!(log.contains(r#""reason":"already_running""#));
        assert!(log.contains(r#""operationId":"op_intelligence_busy""#));
        assert!(log.contains(r#""event":"intelligence-ladder-refresh-finished""#));
        assert!(log.contains(r#""outcome":"succeeded""#));
        assert!(log.contains(r#""entryCount":3"#));
        assert!(log.contains(r#""event":"intelligence-ladder-settings-updated""#));
        assert!(log.contains(r#""operationId":"op_intelligence_settings""#));
        assert!(!log.contains(r#""model":"A""#));
        assert!(!log.contains(r#""iq":"#));
    }
}
