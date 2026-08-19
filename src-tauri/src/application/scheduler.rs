use std::{
    collections::{HashMap, HashSet},
    future::pending,
    panic::AssertUnwindSafe,
    sync::Arc,
    time::Duration,
};

use futures_util::{stream::FuturesUnordered, FutureExt, StreamExt};
use tauri::async_runtime::{self, JoinHandle};
use tokio::{
    sync::{mpsc, watch},
    time::{interval, timeout, MissedTickBehavior},
};
use uuid::Uuid;

use crate::{
    diagnostics::{Diagnostics, ProbeBatchSummary},
    domain::{EntityId, ProbeStatus, TimestampMs},
};

use super::{ApplicationService, ApplicationSnapshot};

const SCHEDULER_TICK: Duration = Duration::from_secs(15);
const RESUME_GAP_MS: i64 = 45_000;
const MAX_CONCURRENT_PROBES: usize = 4;
const SCHEDULED_PROBE_TIMEOUT: Duration = Duration::from_secs(150);
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const HANDLE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(12);
const SNAPSHOT_QUEUE_CAPACITY: usize = 16;
const SNAPSHOT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledModel {
    pub site_id: EntityId,
    pub model_id: EntityId,
    pub interval_minutes: u16,
    pub persisted_next_probe_at: Option<TimestampMs>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSnapshot {
    pub now: TimestampMs,
    pub models: Vec<ScheduledModel>,
}

#[derive(Debug, Clone)]
struct Plan {
    site_id: EntityId,
    interval_minutes: u16,
    due_at: TimestampMs,
}

struct BatchProgress {
    site_count: usize,
    model_count: usize,
    completed_count: usize,
    failed_count: usize,
    skipped_count: usize,
    remaining: usize,
}

enum ProbeAttempt {
    Finished(super::config_service::Result<Option<(ApplicationSnapshot, ProbeStatus)>>),
    TimedOut,
}

#[derive(Clone, Copy)]
enum BatchOutcome {
    Completed,
    Failed,
    Skipped,
}

struct SnapshotDispatcher {
    sender: mpsc::Sender<ApplicationSnapshot>,
    worker: JoinHandle<()>,
    diagnostics: Option<Diagnostics>,
}

impl SnapshotDispatcher {
    fn start(
        on_snapshot: Arc<dyn Fn(ApplicationSnapshot) + Send + Sync>,
        diagnostics: Option<Diagnostics>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::channel(SNAPSHOT_QUEUE_CAPACITY);
        let worker_diagnostics = diagnostics.clone();
        let worker = async_runtime::spawn(async move {
            while let Some(snapshot) = receiver.recv().await {
                let callback = on_snapshot.clone();
                let result = async_runtime::spawn_blocking(move || {
                    std::panic::catch_unwind(AssertUnwindSafe(|| callback(snapshot)))
                })
                .await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        if let Some(diagnostics) = &worker_diagnostics {
                            diagnostics.record_background_error(
                                "scheduler-snapshot-callback",
                                "snapshot_callback_panicked",
                            );
                        }
                    }
                    Err(_) => {
                        if let Some(diagnostics) = &worker_diagnostics {
                            diagnostics.record_background_error(
                                "scheduler-snapshot-callback",
                                "snapshot_callback_worker_failed",
                            );
                        }
                    }
                }
            }
        });
        Self {
            sender,
            worker,
            diagnostics,
        }
    }

    fn dispatch(&self, snapshot: ApplicationSnapshot) {
        if let Err(error) = self.sender.try_send(snapshot) {
            if let Some(diagnostics) = &self.diagnostics {
                let error_code = match error {
                    mpsc::error::TrySendError::Full(_) => "snapshot_queue_full",
                    mpsc::error::TrySendError::Closed(_) => "snapshot_consumer_stopped",
                };
                diagnostics.record_background_error("scheduler-snapshot-delivery", error_code);
            }
        }
    }

    async fn shutdown(self) {
        let Self {
            sender,
            mut worker,
            diagnostics,
        } = self;
        drop(sender);
        match timeout(SNAPSHOT_SHUTDOWN_TIMEOUT, &mut worker).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                if let Some(diagnostics) = &diagnostics {
                    diagnostics.record_background_error(
                        "scheduler-snapshot-callback",
                        "snapshot_consumer_failed",
                    );
                }
            }
            Err(_) => {
                if let Some(diagnostics) = &diagnostics {
                    diagnostics.record_background_error(
                        "scheduler-snapshot-callback",
                        "snapshot_consumer_shutdown_timeout",
                    );
                }
                worker.abort();
                let _ = worker.await;
            }
        }
    }
}

pub struct SchedulerHandle {
    shutdown: watch::Sender<bool>,
    runner: Option<JoinHandle<()>>,
    diagnostics: Option<Diagnostics>,
}

impl SchedulerHandle {
    pub async fn shutdown(mut self) -> bool {
        let _ = self.shutdown.send(true);
        let Some(mut runner) = self.runner.take() else {
            return true;
        };
        match timeout(HANDLE_SHUTDOWN_TIMEOUT, &mut runner).await {
            Ok(Ok(())) => true,
            Ok(Err(_)) => {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics
                        .record_background_error("scheduler-shutdown", "scheduler_runner_failed");
                }
                false
            }
            Err(_) => {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_background_error(
                        "scheduler-shutdown",
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

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(runner) = self.runner.take() {
            runner.abort();
        }
    }
}

pub fn start_scheduler<F>(service: ApplicationService, on_snapshot: F) -> SchedulerHandle
where
    F: Fn(ApplicationSnapshot) + Send + Sync + 'static,
{
    let (shutdown, receiver) = watch::channel(false);
    let diagnostics = service.diagnostics().cloned();
    let runner_diagnostics = diagnostics.clone();
    let runner = async_runtime::spawn(async move {
        if AssertUnwindSafe(run_scheduler(service, receiver, Arc::new(on_snapshot)))
            .catch_unwind()
            .await
            .is_err()
        {
            if let Some(diagnostics) = runner_diagnostics {
                diagnostics.record_background_error("scheduler-runner", "scheduler_panicked");
            }
        }
    });
    SchedulerHandle {
        shutdown,
        runner: Some(runner),
        diagnostics,
    }
}

async fn run_scheduler(
    service: ApplicationService,
    mut shutdown: watch::Receiver<bool>,
    on_snapshot: Arc<dyn Fn(ApplicationSnapshot) + Send + Sync>,
) {
    let snapshot_dispatcher =
        SnapshotDispatcher::start(on_snapshot, service.diagnostics().cloned());
    let mut ticker = interval(SCHEDULER_TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut plans = HashMap::<EntityId, Plan>::new();
    let mut active = HashSet::<EntityId>::new();
    let mut probes = FuturesUnordered::new();
    let mut batches = HashMap::<String, BatchProgress>::new();
    let mut last_tick = None;

    loop {
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            _ = ticker.tick() => {
                let snapshot = match service.schedule_snapshot().await {
                    Ok(snapshot) => snapshot,
                    Err(_) => {
                        if let Some(diagnostics) = service.diagnostics() {
                            diagnostics.record_background_error(
                                "scheduler-snapshot",
                                "schedule_snapshot_failed",
                            );
                        }
                        continue;
                    }
                };
                let resumed = last_tick.is_some_and(|previous: TimestampMs| {
                    snapshot.now.as_i64().saturating_sub(previous.as_i64()) > RESUME_GAP_MS
                });
                last_tick = Some(snapshot.now);
                reconcile_plans(&mut plans, &snapshot, resumed);

                let due_models = select_due_models(
                    &plans,
                    &active,
                    snapshot.now,
                    MAX_CONCURRENT_PROBES.saturating_sub(active.len()),
                );

                let batch_id = format!("batch_{}", Uuid::new_v4().simple());
                if !due_models.is_empty() {
                    let site_count = due_models
                        .iter()
                        .map(|(_, site_id, _)| site_id.clone())
                        .collect::<HashSet<_>>()
                        .len();
                    batches.insert(
                        batch_id.clone(),
                        BatchProgress {
                            site_count,
                            model_count: due_models.len(),
                            completed_count: 0,
                            failed_count: 0,
                            skipped_count: 0,
                            remaining: due_models.len(),
                        },
                    );
                }

                for (model_id, site_id, interval_minutes) in due_models {
                    if let Some(plan) = plans.get_mut(&model_id) {
                        plan.due_at = stable_next_probe_at(
                            snapshot.now,
                            &site_id,
                            &model_id,
                            interval_minutes,
                        );
                    }
                    active.insert(model_id.clone());
                    let probe_service = service.clone();
                    let probe_batch_id = batch_id.clone();
                    probes.push(async move {
                        let result = match timeout(
                            SCHEDULED_PROBE_TIMEOUT,
                            probe_service.probe_scheduled_model(site_id, model_id.clone()),
                        )
                        .await
                        {
                            Ok(result) => ProbeAttempt::Finished(result),
                            Err(_) => ProbeAttempt::TimedOut,
                        };
                        (model_id, probe_batch_id, result)
                    });
                }
            }
            completed = async {
                if probes.is_empty() {
                    pending().await
                } else {
                    probes.next().await
                }
            } => {
                if let Some(completed) = completed {
                    finish_probe(
                        &service,
                        &snapshot_dispatcher,
                        &mut active,
                        &mut batches,
                        completed,
                    );
                }
            }
        }
    }

    let drained = timeout(SHUTDOWN_DRAIN_TIMEOUT, async {
        while let Some(completed) = probes.next().await {
            finish_probe(
                &service,
                &snapshot_dispatcher,
                &mut active,
                &mut batches,
                completed,
            );
        }
    })
    .await
    .is_ok();
    if !drained {
        if let Some(diagnostics) = service.diagnostics() {
            diagnostics.record_background_error("scheduler-shutdown", "scheduled_probes_cancelled");
        }
        for batch in batches.values_mut() {
            batch.failed_count = batch.failed_count.saturating_add(batch.remaining);
            batch.remaining = 0;
        }
    }
    drop(probes);
    for (batch_id, batch) in batches.drain() {
        record_batch_finished(&service, &batch_id, batch);
    }
    snapshot_dispatcher.shutdown().await;
}

fn finish_probe(
    service: &ApplicationService,
    snapshot_dispatcher: &SnapshotDispatcher,
    active: &mut HashSet<EntityId>,
    batches: &mut HashMap<String, BatchProgress>,
    (model_id, batch_id, result): (EntityId, String, ProbeAttempt),
) {
    active.remove(&model_id);
    let outcome = match result {
        ProbeAttempt::Finished(Ok(Some((snapshot, ProbeStatus::Healthy | ProbeStatus::Slow)))) => {
            snapshot_dispatcher.dispatch(snapshot);
            BatchOutcome::Completed
        }
        ProbeAttempt::Finished(Ok(Some((snapshot, _)))) => {
            snapshot_dispatcher.dispatch(snapshot);
            BatchOutcome::Failed
        }
        ProbeAttempt::Finished(Ok(None)) => BatchOutcome::Skipped,
        ProbeAttempt::Finished(Err(_)) => {
            if let Some(diagnostics) = service.diagnostics() {
                diagnostics.record_background_error("scheduled-probe", "scheduled_probe_failed");
            }
            BatchOutcome::Failed
        }
        ProbeAttempt::TimedOut => {
            if let Some(diagnostics) = service.diagnostics() {
                diagnostics.record_background_error("scheduled-probe", "scheduled_probe_timeout");
            }
            BatchOutcome::Failed
        }
    };
    let finished = if let Some(batch) = batches.get_mut(&batch_id) {
        match outcome {
            BatchOutcome::Completed => {
                batch.completed_count = batch.completed_count.saturating_add(1)
            }
            BatchOutcome::Failed => batch.failed_count = batch.failed_count.saturating_add(1),
            BatchOutcome::Skipped => batch.skipped_count = batch.skipped_count.saturating_add(1),
        }
        batch.remaining = batch.remaining.saturating_sub(1);
        batch.remaining == 0
    } else {
        false
    };
    if finished {
        let batch = batches.remove(&batch_id).expect("finished batch exists");
        record_batch_finished(service, &batch_id, batch);
    }
}

fn record_batch_finished(service: &ApplicationService, batch_id: &str, batch: BatchProgress) {
    if let Some(diagnostics) = service.diagnostics() {
        diagnostics.record_probe_batch_finished(
            Some(batch_id),
            "automatic",
            ProbeBatchSummary {
                site_count: u64::try_from(batch.site_count).unwrap_or(u64::MAX),
                model_count: u64::try_from(batch.model_count).unwrap_or(u64::MAX),
                completed_count: u64::try_from(batch.completed_count).unwrap_or(u64::MAX),
                failed_count: u64::try_from(batch.failed_count).unwrap_or(u64::MAX),
                skipped_count: u64::try_from(batch.skipped_count).unwrap_or(u64::MAX),
            },
        );
    }
}

fn reconcile_plans(
    plans: &mut HashMap<EntityId, Plan>,
    snapshot: &ScheduleSnapshot,
    resumed: bool,
) {
    let configured = snapshot
        .models
        .iter()
        .map(|model| model.model_id.clone())
        .collect::<HashSet<_>>();
    plans.retain(|model_id, _| configured.contains(model_id));

    for model in &snapshot.models {
        let plan_changed = plans.get(&model.model_id).is_some_and(|plan| {
            plan.interval_minutes != model.interval_minutes || plan.site_id != model.site_id
        });
        let needs_recompute = resumed || plan_changed;
        if needs_recompute {
            plans.remove(&model.model_id);
        }
        plans.entry(model.model_id.clone()).or_insert_with(|| {
            let due_at = (!plan_changed)
                .then_some(model.persisted_next_probe_at)
                .flatten()
                .filter(|due_at| *due_at > snapshot.now)
                .unwrap_or_else(|| {
                    stable_next_probe_at(
                        snapshot.now,
                        &model.site_id,
                        &model.model_id,
                        model.interval_minutes,
                    )
                });
            Plan {
                site_id: model.site_id.clone(),
                interval_minutes: model.interval_minutes,
                due_at,
            }
        });
    }
}

fn select_due_models(
    plans: &HashMap<EntityId, Plan>,
    active: &HashSet<EntityId>,
    now: TimestampMs,
    limit: usize,
) -> Vec<(EntityId, EntityId, u16)> {
    let mut due_models = plans
        .iter()
        .filter(|(model_id, plan)| plan.due_at <= now && !active.contains(*model_id))
        .map(|(model_id, plan)| {
            (
                plan.due_at,
                model_id.clone(),
                plan.site_id.clone(),
                plan.interval_minutes,
            )
        })
        .collect::<Vec<_>>();
    due_models.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    due_models.truncate(limit);
    due_models
        .into_iter()
        .map(|(_, model_id, site_id, interval_minutes)| (model_id, site_id, interval_minutes))
        .collect()
}

pub fn stable_next_probe_at(
    after: TimestampMs,
    site_id: &EntityId,
    model_id: &EntityId,
    interval_minutes: u16,
) -> TimestampMs {
    let interval_ms = i64::from(interval_minutes).saturating_mul(60_000).max(1);
    let offset =
        i64::try_from(stable_hash(site_id, model_id) % interval_ms as u64).unwrap_or_default();
    let period_start = after.as_i64() - after.as_i64().rem_euclid(interval_ms);
    let candidate = period_start.saturating_add(offset);
    let next = if candidate > after.as_i64() {
        candidate
    } else {
        candidate.saturating_add(interval_ms)
    };
    TimestampMs::new(next).unwrap_or(after)
}

fn stable_hash(site_id: &EntityId, model_id: &EntityId) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    site_id
        .as_str()
        .bytes()
        .chain([0xff])
        .chain(model_id.as_str().bytes())
        .fold(OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
}

pub(crate) fn is_failure(status: ProbeStatus) -> bool {
    !matches!(
        status,
        ProbeStatus::Healthy | ProbeStatus::Slow | ProbeStatus::Unknown
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    fn id(value: &str) -> EntityId {
        EntityId::parse(value).expect("fixture id is valid")
    }

    fn timestamp(value: i64) -> TimestampMs {
        TimestampMs::new(value).expect("fixture timestamp is valid")
    }

    #[test]
    fn stable_stagger_is_reproducible_and_strictly_future() {
        let now = timestamp(1_700_000_000_000);
        let first = stable_next_probe_at(now, &id("site-a"), &id("model-a"), 10);
        let repeated = stable_next_probe_at(now, &id("site-a"), &id("model-a"), 10);
        let other = stable_next_probe_at(now, &id("site-a"), &id("model-b"), 10);

        assert_eq!(first, repeated);
        assert!(first > now);
        assert_ne!(first, other);
        assert!(first.as_i64() - now.as_i64() <= 600_000);
    }

    #[test]
    fn resume_discards_overdue_work_instead_of_backfilling() {
        let now = timestamp(1_700_000_000_000);
        let model = ScheduledModel {
            site_id: id("site-a"),
            model_id: id("model-a"),
            interval_minutes: 10,
            persisted_next_probe_at: Some(timestamp(now.as_i64() - 60_000)),
        };
        let mut plans = HashMap::from([(
            model.model_id.clone(),
            Plan {
                site_id: model.site_id.clone(),
                interval_minutes: 10,
                due_at: timestamp(now.as_i64() - 60_000),
            },
        )]);

        reconcile_plans(
            &mut plans,
            &ScheduleSnapshot {
                now,
                models: vec![model],
            },
            true,
        );

        assert!(plans[&id("model-a")].due_at > now);
    }

    #[test]
    fn interval_change_discards_the_persisted_schedule() {
        let now = timestamp(1_700_000_000_000);
        let model = ScheduledModel {
            site_id: id("site-a"),
            model_id: id("model-a"),
            interval_minutes: 1,
            persisted_next_probe_at: Some(timestamp(now.as_i64() + 600_000)),
        };
        let mut plans = HashMap::from([(
            model.model_id.clone(),
            Plan {
                site_id: model.site_id.clone(),
                interval_minutes: 10,
                due_at: timestamp(now.as_i64() + 600_000),
            },
        )]);

        reconcile_plans(
            &mut plans,
            &ScheduleSnapshot {
                now,
                models: vec![model],
            },
            false,
        );

        let due_at = plans[&id("model-a")].due_at;
        assert!(due_at > now);
        assert!(due_at.as_i64() - now.as_i64() <= 60_000);
    }

    #[test]
    fn site_change_replaces_the_plan_and_discards_the_persisted_schedule() {
        let now = timestamp(1_700_000_000_000);
        let model = ScheduledModel {
            site_id: id("site-b"),
            model_id: id("model-a"),
            interval_minutes: 10,
            persisted_next_probe_at: Some(timestamp(now.as_i64() + 3_600_000)),
        };
        let mut plans = HashMap::from([(
            model.model_id.clone(),
            Plan {
                site_id: id("site-a"),
                interval_minutes: 10,
                due_at: timestamp(now.as_i64() + 3_600_000),
            },
        )]);

        reconcile_plans(
            &mut plans,
            &ScheduleSnapshot {
                now,
                models: vec![model],
            },
            false,
        );

        let plan = &plans[&id("model-a")];
        assert_eq!(plan.site_id, id("site-b"));
        assert!(plan.due_at > now);
        assert!(plan.due_at.as_i64() - now.as_i64() <= 600_000);
    }

    #[test]
    fn reconciliation_removes_disabled_or_deleted_models() {
        let now = timestamp(1_700_000_000_000);
        let mut plans = HashMap::from([(
            id("model-a"),
            Plan {
                site_id: id("site-a"),
                interval_minutes: 10,
                due_at: timestamp(now.as_i64() + 60_000),
            },
        )]);

        reconcile_plans(
            &mut plans,
            &ScheduleSnapshot {
                now,
                models: Vec::new(),
            },
            false,
        );

        assert!(plans.is_empty());
    }

    #[test]
    fn due_model_selection_is_bounded_and_prefers_the_oldest_work() {
        let now = timestamp(1_700_000_000_000);
        let plans = (0..6)
            .map(|index| {
                let model_id = id(&format!("model-{index}"));
                (
                    model_id,
                    Plan {
                        site_id: id("site-a"),
                        interval_minutes: 10,
                        due_at: timestamp(now.as_i64() - i64::from(6 - index)),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let active = HashSet::from([id("model-0")]);

        let selected = select_due_models(&plans, &active, now, 2);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].0, id("model-1"));
        assert_eq!(selected[1].0, id("model-2"));
    }

    #[tokio::test]
    async fn explicit_shutdown_waits_for_runner_cleanup() {
        let (shutdown, mut receiver) = watch::channel(false);
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let runner_cleaned_up = Arc::clone(&cleaned_up);
        let runner = async_runtime::spawn(async move {
            let _ = receiver.changed().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            runner_cleaned_up.store(true, Ordering::Release);
        });
        let handle = SchedulerHandle {
            shutdown,
            runner: Some(runner),
            diagnostics: None,
        };

        handle.shutdown().await;

        assert!(cleaned_up.load(Ordering::Acquire));
    }
}
