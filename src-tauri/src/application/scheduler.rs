use std::{
    collections::{HashMap, HashSet},
    future::pending,
    sync::Arc,
    time::Duration,
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use tauri::async_runtime::{self, JoinHandle};
use tokio::{
    sync::watch,
    time::{interval, MissedTickBehavior},
};
use uuid::Uuid;

use crate::{
    diagnostics::ProbeBatchSummary,
    domain::{EntityId, LatestModelStatus, ProbeStatus, ProbeTimingPolicy, TimestampMs},
};

use super::{ApplicationService, ApplicationSnapshot};

const SCHEDULER_TICK: Duration = Duration::from_secs(15);
const RESUME_GAP_MS: i64 = 45_000;

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

pub struct SchedulerHandle {
    shutdown: watch::Sender<bool>,
    runner: JoinHandle<()>,
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.runner.abort();
    }
}

pub fn start_scheduler<F>(service: ApplicationService, on_snapshot: F) -> SchedulerHandle
where
    F: Fn(ApplicationSnapshot) + Send + Sync + 'static,
{
    let (shutdown, receiver) = watch::channel(false);
    let runner = async_runtime::spawn(run_scheduler(service, receiver, Arc::new(on_snapshot)));
    SchedulerHandle { shutdown, runner }
}

async fn run_scheduler(
    service: ApplicationService,
    mut shutdown: watch::Receiver<bool>,
    on_snapshot: Arc<dyn Fn(ApplicationSnapshot) + Send + Sync>,
) {
    let mut ticker = interval(SCHEDULER_TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut plans = HashMap::<EntityId, Plan>::new();
    let mut active = HashSet::<EntityId>::new();
    let mut probes = FuturesUnordered::new();
    let mut batches = HashMap::<String, BatchProgress>::new();
    let mut last_tick = None;

    loop {
        tokio::select! {
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

                let due_models = plans
                    .iter()
                    .filter(|(model_id, plan)| {
                        plan.due_at <= snapshot.now && !active.contains(*model_id)
                    })
                    .map(|(model_id, plan)| {
                        (model_id.clone(), plan.site_id.clone(), plan.interval_minutes)
                    })
                    .collect::<Vec<_>>();

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
                        let result = probe_service
                            .probe_scheduled_model(site_id, model_id.clone())
                            .await;
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
                if let Some((model_id, batch_id, result)) = completed {
                    active.remove(&model_id);
                    let outcome = match result {
                        Ok(Some((snapshot, ProbeStatus::Healthy | ProbeStatus::Slow))) => {
                            on_snapshot(snapshot);
                            "completed"
                        }
                        Ok(Some((snapshot, _))) => {
                            on_snapshot(snapshot);
                            "failed"
                        }
                        Ok(None) => "skipped",
                        Err(_) => {
                            if let Some(diagnostics) = service.diagnostics() {
                                diagnostics.record_background_error(
                                    "scheduled-probe",
                                    "scheduled_probe_failed",
                                );
                            }
                            "failed"
                        }
                    };
                    let finished = if let Some(batch) = batches.get_mut(&batch_id) {
                        match outcome {
                            "completed" => {
                                batch.completed_count = batch.completed_count.saturating_add(1)
                            }
                            "failed" => batch.failed_count = batch.failed_count.saturating_add(1),
                            _ => batch.skipped_count = batch.skipped_count.saturating_add(1),
                        }
                        batch.remaining = batch.remaining.saturating_sub(1);
                        batch.remaining == 0
                    } else {
                        false
                    };
                    if finished {
                        let batch = batches.remove(&batch_id).expect("finished batch exists");
                        if let Some(diagnostics) = service.diagnostics() {
                            diagnostics.record_probe_batch_finished(
                                Some(&batch_id),
                                "automatic",
                                ProbeBatchSummary {
                                    site_count: u64::try_from(batch.site_count)
                                        .unwrap_or(u64::MAX),
                                    model_count: u64::try_from(batch.model_count)
                                        .unwrap_or(u64::MAX),
                                    completed_count: u64::try_from(batch.completed_count)
                                        .unwrap_or(u64::MAX),
                                    failed_count: u64::try_from(batch.failed_count)
                                        .unwrap_or(u64::MAX),
                                    skipped_count: u64::try_from(batch.skipped_count)
                                        .unwrap_or(u64::MAX),
                                },
                            );
                        }
                    }
                }
            }
        }
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
        let interval_changed = plans
            .get(&model.model_id)
            .is_some_and(|plan| plan.interval_minutes != model.interval_minutes);
        let needs_recompute = resumed || interval_changed;
        if needs_recompute {
            plans.remove(&model.model_id);
        }
        plans.entry(model.model_id.clone()).or_insert_with(|| {
            let due_at = (!interval_changed)
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

pub fn aggregate_latest_status(
    previous: Option<&LatestModelStatus>,
    observed: ProbeStatus,
    timing: &ProbeTimingPolicy,
    model_id: EntityId,
    probe_result_id: EntityId,
    finished_at: TimestampMs,
    next_probe_at: Option<TimestampMs>,
) -> LatestModelStatus {
    let success = matches!(observed, ProbeStatus::Healthy | ProbeStatus::Slow);
    let consecutive_failures = if success {
        0
    } else {
        previous.map_or(1, |status| status.consecutive_failures.saturating_add(1))
    };
    let consecutive_successes = if success {
        previous.map_or(1, |status| status.consecutive_successes.saturating_add(1))
    } else {
        0
    };
    let status = if success {
        if previous.is_some_and(|status| is_failure(status.status))
            && consecutive_successes < u16::from(timing.recovery_threshold)
        {
            previous.map_or(observed, |status| status.status)
        } else {
            observed
        }
    } else if consecutive_failures < u16::from(timing.failure_threshold) {
        previous.map_or(ProbeStatus::Unknown, |status| status.status)
    } else {
        observed
    };

    LatestModelStatus {
        model_id,
        probe_result_id: Some(probe_result_id),
        status,
        consecutive_failures,
        consecutive_successes,
        next_probe_at,
        revision: 0,
        updated_at: finished_at,
    }
}

fn is_failure(status: ProbeStatus) -> bool {
    !matches!(
        status,
        ProbeStatus::Healthy | ProbeStatus::Slow | ProbeStatus::Unknown
    )
}

#[cfg(test)]
mod tests {
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
    fn failure_and_recovery_thresholds_prevent_single_result_flapping() {
        let timing = ProbeTimingPolicy::default();
        let first_failure = aggregate_latest_status(
            None,
            ProbeStatus::Timeout,
            &timing,
            id("model-a"),
            id("result-1"),
            timestamp(100),
            None,
        );
        assert_eq!(first_failure.status, ProbeStatus::Unknown);

        let second_failure = aggregate_latest_status(
            Some(&first_failure),
            ProbeStatus::Timeout,
            &timing,
            id("model-a"),
            id("result-2"),
            timestamp(200),
            None,
        );
        assert_eq!(second_failure.status, ProbeStatus::Timeout);

        let first_recovery = aggregate_latest_status(
            Some(&second_failure),
            ProbeStatus::Healthy,
            &timing,
            id("model-a"),
            id("result-3"),
            timestamp(300),
            None,
        );
        assert_eq!(first_recovery.status, ProbeStatus::Timeout);

        let second_recovery = aggregate_latest_status(
            Some(&first_recovery),
            ProbeStatus::Healthy,
            &timing,
            id("model-a"),
            id("result-4"),
            timestamp(400),
            None,
        );
        assert_eq!(second_recovery.status, ProbeStatus::Healthy);
    }
}
