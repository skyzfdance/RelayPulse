use super::{
    activity::{ProbeActivity, ProbeTask},
    health::{self, HealthState},
    records::{capture::CaptureStore, CompletedRecord, RecordContext},
    sqlite::MonitoringStore,
    transitions::{HealthChange, HealthObserver},
};
use crate::{
    modules::{
        configuration::ConfigurationService,
        sites::{Site, SiteError, SiteService, TaskError},
    },
    shared::{
        diagnostics::{DiagnosticFailure, Diagnostics, Failure, Recorder},
        file_output::OutputError,
        model_client::{Cancellation, ExecutionOptions, ModelClient},
        persistence::Database,
        state_changes::StateChanges,
    },
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
#[derive(Debug, thiserror::Error)]
pub(crate) enum MonitoringError {
    #[error(transparent)]
    Site(#[from] SiteError),
    #[error("正式监测记录不可用。以下操作未确认完成。")]
    Storage,
    #[error("本次探测的记录文件写入失败。")]
    Capture,
    #[error("正式监测任务执行失败。")]
    Worker,
}
impl DiagnosticFailure for MonitoringError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::Site(error) => error.diagnostic_failure(),
            Self::Storage | Self::Capture => Failure::Storage,
            Self::Worker => Failure::Platform,
        }
    }
}
type Result<T> = std::result::Result<T, MonitoringError>;
pub(crate) struct Monitoring {
    pub(super) sites: Arc<SiteService>,
    client: Arc<ModelClient>,
    pub(super) storage: MonitoringStore,
    pub(super) captures: Option<CaptureStore>,
    pub(super) general: Arc<ConfigurationService>,
    pub(super) diagnostics: Recorder,
    pub(super) record_access: Arc<tokio::sync::RwLock<()>>,
    scheduler_started: AtomicBool,
    changes: Arc<StateChanges>,
    activity: Arc<Mutex<ProbeActivity>>,
    health_observer: OnceLock<Arc<dyn HealthObserver>>,
}
impl Monitoring {
    pub(crate) fn new(
        sites: Arc<SiteService>,
        client: Arc<ModelClient>,
        database: Arc<Database>,
        general: Arc<ConfigurationService>,
        diagnostics: Arc<Diagnostics>,
        capture_root: Option<std::path::PathBuf>,
    ) -> Self {
        let changes = sites.changes();
        Self {
            sites,
            client,
            storage: MonitoringStore::new(database),
            captures: capture_root.map(CaptureStore::new),
            general,
            diagnostics: Recorder::new(diagnostics),
            record_access: Arc::new(tokio::sync::RwLock::new(())),
            scheduler_started: AtomicBool::new(false),
            changes,
            activity: Arc::default(),
            health_observer: OnceLock::new(),
        }
    }
    /// 宿主在启动调度器前连接一次；模块独立运行时不依赖系统通知。
    pub(crate) fn set_health_observer(
        &self,
        observer: Arc<dyn HealthObserver>,
    ) -> std::result::Result<(), Arc<dyn HealthObserver>> {
        self.health_observer.set(observer)
    }
    pub(crate) fn activity(&self) -> ProbeActivity {
        self.activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.sites.cancellation()
    }
    pub(crate) fn close(&self) {
        self.sites.close();
        if let Some(captures) = &self.captures {
            captures.close();
        }
    }
    pub(crate) async fn drained(&self) {
        self.sites.drained().await;
        self.storage.drained().await;
        if let Some(captures) = &self.captures {
            captures.drained().await;
        }
    }
    pub(crate) fn claim_scheduler(&self) -> std::result::Result<(), TaskError> {
        if *self.cancellation().subscribe().borrow() {
            return Err(TaskError::Closing);
        }
        self.scheduler_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| TaskError::Busy)
    }
    pub(crate) fn scheduler_failed(&self) {
        let _ = self
            .diagnostics
            .observe("site_scheduler", || Err::<(), _>(MonitoringError::Worker));
    }
    pub(super) fn settings_changes(&self) -> tokio::sync::watch::Receiver<()> {
        self.general.subscribe()
    }
    pub(crate) async fn automatic_enabled(&self) -> Result<bool> {
        let general = self.general.clone();
        self.diagnostics
            .observe_async("site_schedule_settings", async {
                self.storage
                    .blocking(move || general.load())
                    .await
                    .map_err(|_| MonitoringError::Worker)?
                    .map(|settings| settings.continue_in_background)
                    .map_err(|_| MonitoringError::Storage)
            })
            .await
    }
    pub(crate) async fn states(
        &self,
        model_ids: Vec<String>,
    ) -> Result<HashMap<String, HealthState>> {
        let storage = self.storage.clone();
        self.storage
            .blocking(move || storage.states(&model_ids))
            .await
            .map_err(|_| MonitoringError::Worker)?
            .map_err(|_| MonitoringError::Storage)
    }
    pub(crate) async fn run(&self, site_id: String, model_id: Option<String>) -> Result<Site> {
        self.run_guarded(site_id, model_id, false).await
    }
    async fn run_guarded(
        &self,
        site_id: String,
        model_id: Option<String>,
        scheduled: bool,
    ) -> Result<Site> {
        let cancellation = self.cancellation();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(SiteError::Task(TaskError::Closing).into()),
            result = self.perform(site_id, model_id, scheduled) => result,
        }
    }
    async fn perform(
        &self,
        site_id: String,
        model_id: Option<String>,
        scheduled: bool,
    ) -> Result<Site> {
        let scope = if scheduled {
            self.sites
                .begin_scheduled_execution(site_id.clone())
                .await?
        } else {
            self.sites.begin_execution(site_id.clone()).await?
        };
        if let Some(id) = &model_id {
            scope.resolve_probe_input(id).map_err(SiteError::from)?;
        }
        let ids = scope
            .models()
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>();
        let storage = self.storage.clone();
        let general = self.general.clone();
        let (mut states, retention) = scope
            .blocking(move || {
                let states = storage.states(&ids).map_err(|_| MonitoringError::Storage)?;
                let retention = general
                    .load()
                    .map_err(|_| MonitoringError::Storage)?
                    .history_retention_days;
                Ok::<_, MonitoringError>((states, retention))
            })
            .await??;
        let planned = scope
            .models()
            .iter()
            .filter(|model| {
                let selected = model_id
                    .as_ref()
                    .map_or(model.enabled, |id| id == &model.id);
                selected
                    && (!scheduled
                        || health::due(
                            states.get(&model.id).map_or(model.created_at_ms, |state| {
                                state.finished_at_ms.max(model.created_at_ms)
                            }),
                            scope.site().connection.interval_seconds,
                            health::now_ms(),
                        ))
            })
            .map(|model| model.id.clone())
            .collect::<Vec<_>>();
        let mut task = (!planned.is_empty()).then(|| {
            ProbeTask::start(
                self.activity.clone(),
                self.changes.clone(),
                site_id.clone(),
                planned,
                states
                    .iter()
                    .map(|(id, state)| (id.clone(), state.finished_at_ms))
                    .collect(),
            )
        });
        for model in scope.models() {
            // 等待清理结束后，再校验自动开关和到期规则，不使用等待之前的派发判断。
            let history = Arc::new(self.record_access.clone().read_owned().await);
            let enabled = if scheduled {
                // 每个请求派发前读取开关；已完成的请求照常落库，不因关闭开关而取消。
                let current = self.sites.get(site_id.clone()).await?;
                if !current.enabled || !self.automatic_enabled().await? {
                    break;
                }
                current.models.iter().any(|m| m.id == model.id && m.enabled)
            } else {
                model_id
                    .as_ref()
                    .map_or(model.enabled, |id| id == &model.id)
            };
            if !enabled {
                continue;
            }
            let previous = states.get(&model.id).cloned().unwrap_or_default();
            if scheduled
                && !health::due(
                    previous.finished_at_ms.max(model.created_at_ms),
                    scope.site().connection.interval_seconds,
                    health::now_ms(),
                )
            {
                continue;
            }
            let current_task = task.get_or_insert_with(|| {
                ProbeTask::start(
                    self.activity.clone(),
                    self.changes.clone(),
                    site_id.clone(),
                    vec![],
                    states
                        .iter()
                        .map(|(id, state)| (id.clone(), state.finished_at_ms))
                        .collect(),
                )
            });
            current_task.include(&model.id);
            // 正式采集、提交和发布期间不允许手动删除历史；取消后的事务仍持有此租约。
            let request = scope
                .resolve_probe_input(&model.id)
                .map_err(SiteError::from)?;
            let context = RecordContext::new(
                scope.site(),
                model,
                &request,
                scheduled,
                self.captures.is_some(),
            )
            .map_err(crate::shared::model_client::ClientError::from)
            .map_err(SiteError::from)?;
            let mut recording = if let Some(captures) = &self.captures {
                Some(
                    captures
                        .start(
                            context.id.clone(),
                            &site_id,
                            &model.id,
                            &request.target.secret,
                        )
                        .await
                        .map_err(|_| MonitoringError::Capture)?,
                )
            } else {
                None
            };
            if let Some(recording) = recording.as_mut() {
                recording
                    .context(&context)
                    .await
                    .map_err(|_| MonitoringError::Capture)?;
            }
            let capture = recording
                .as_mut()
                .map(|recording| recording as &mut dyn crate::shared::model_client::CaptureSink);
            let result = self
                .client
                .check_once(
                    &request,
                    ExecutionOptions::default(),
                    &self.cancellation(),
                    capture,
                )
                .await
                .map_err(SiteError::from)?;
            let next = health::advance(
                &previous,
                &result,
                &scope.site().connection,
                health::now_ms(),
            );
            let storage = self.storage.clone();
            let saved = next.clone();
            let record =
                CompletedRecord::new(context, result, &request.target.secret, next.finished_at_ms);
            let held_history = history.clone();
            let changes = self.changes.clone();
            let held_task = current_task.clone();
            let completed_id = model.id.clone();
            let health_change =
                HealthChange::between(&previous, &next, &model.name, &request.target.base_url);
            let observer = self.health_observer.get().cloned();
            scope
                .blocking(move || {
                    {
                        let _history = held_history;
                        let _write = changes.begin();
                        storage.complete(&record, &saved, retention)?;
                        held_task.completed(completed_id);
                    }
                    if let (Some(observer), Some(change)) = (observer, health_change) {
                        observer.changed(&change);
                    }
                    Ok::<_, crate::shared::persistence::PersistenceError>(())
                })
                .await?
                .map_err(|_| MonitoringError::Storage)?;
            if let Some(recording) = recording {
                recording
                    .publish()
                    .await
                    .map_err(|_| MonitoringError::Capture)?;
            }
            states.insert(model.id.clone(), next);
        }
        Ok(scope.site().clone())
    }
    pub(crate) async fn cleanup_records(&self) {
        let access = Arc::new(self.record_access.clone().read_owned().await);
        let general = self.general.clone();
        let records = self.storage.records();
        let result = async {
            let history = records.clone();
            let held_access = access.clone();
            let retention = self
                .storage
                .blocking(move || {
                    let _access = held_access;
                    let retention = general
                        .load()
                        .map_err(|_| MonitoringError::Storage)?
                        .history_retention_days;
                    history
                        .prune(health::now_ms().saturating_sub(u64::from(retention) * 86_400_000))
                        .map_err(|_| MonitoringError::Storage)?;
                    Ok::<_, MonitoringError>(retention)
                })
                .await
                .map_err(|_| MonitoringError::Worker)??;
            if let Some(captures) = &self.captures {
                // Future 在文件枚举之后才执行；所有 SQLite worker 都纳入退出等待。
                let retained = async {
                    self.storage
                        .blocking(move || records.retained_ids())
                        .await
                        .map_err(|_| OutputError)?
                        .map_err(|_| OutputError)
                };
                captures
                    .cleanup(retained, retention)
                    .await
                    .map_err(|_| MonitoringError::Capture)?;
            }
            Ok::<_, MonitoringError>(())
        }
        .await;
        if let Err(error) = result {
            let _ = self
                .diagnostics
                .observe("probe_record_cleanup", || Err::<(), _>(error));
        }
        drop(access);
    }
    pub(super) async fn due_sites(&self) -> Vec<String> {
        let result = async {
            let sites: Vec<_> = self
                .sites
                .list()
                .await?
                .into_iter()
                .filter(|site| site.enabled && site.models.iter().any(|model| model.enabled))
                .collect();
            if sites.is_empty() {
                return Ok(vec![]);
            }
            let ids = sites
                .iter()
                .flat_map(|site| site.models.iter().map(|m| m.id.clone()))
                .collect();
            let states = self.states(ids).await?;
            Ok::<_, MonitoringError>(
                sites
                    .into_iter()
                    .filter(|site| {
                        site.models.iter().any(|model| {
                            model.enabled
                                && health::due(
                                    states.get(&model.id).map_or(model.created_at_ms, |state| {
                                        state.finished_at_ms.max(model.created_at_ms)
                                    }),
                                    site.connection.interval_seconds,
                                    health::now_ms(),
                                )
                        })
                    })
                    .map(|site| site.id)
                    .collect(),
            )
        }
        .await;
        match result {
            Ok(ids) => ids,
            Err(error) => {
                let _ = self
                    .diagnostics
                    .observe("site_schedule_scan", || Err::<(), _>(error));
                vec![]
            }
        }
    }
    pub(super) async fn run_scheduled(&self, id: String) {
        // 忙碌是周期扫描的正常跳过，不记录成失败；作用域仍由唯一站点协调器取得。
        let result = self.run_guarded(id, None, true).await;
        if !matches!(
            result,
            Err(MonitoringError::Site(SiteError::Task(TaskError::Busy)))
        ) {
            let _ = self.diagnostics.observe("site_probe_scheduled", || result);
        }
    }
}
