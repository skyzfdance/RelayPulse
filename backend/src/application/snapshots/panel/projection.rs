use super::{types::*, PanelError};
use crate::modules::{
    configuration::ThemeMode,
    monitoring::{next_due_at, Health, HealthState, ProbeActivity},
    sites::Site,
};
use std::collections::{HashMap, HashSet};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

fn timestamp(ms: u64) -> Result<String, PanelError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map_err(|_| PanelError::Timestamp)?
        .format(&Rfc3339)
        .map_err(|_| PanelError::Timestamp)
}

pub(super) fn project(
    revision: u64,
    sites: &[Site],
    states: &HashMap<String, HealthState>,
    activity: &ProbeActivity,
    automatic: bool,
    theme: ThemeMode,
) -> Result<PanelSnapshot, PanelError> {
    let eligible: Vec<_> = sites.iter().filter(|site| site.enabled).collect();
    let model_ids: HashSet<_> = eligible
        .iter()
        .flat_map(|site| {
            site.models
                .iter()
                .filter(|model| model.enabled)
                .map(|model| model.id.as_str())
        })
        .collect();
    let groups = eligible
        .iter()
        .filter_map(|site| {
            let models = site
                .models
                .iter()
                .filter(|model| model.enabled)
                .filter_map(|model| {
                    let state = states.get(&model.id)?;
                    let status = match state.health {
                        Health::Healthy => PanelModelStatus::Healthy,
                        Health::Warning => PanelModelStatus::Slow,
                        Health::Error => PanelModelStatus::Unavailable,
                        // 已有结果但尚未达到判定阈值，也要展示模型和实际耗时。
                        Health::Muted if state.finished_at_ms != 0 => PanelModelStatus::Pending,
                        // 真正未完成过探测时才不生成结果行。
                        Health::Muted => return None,
                    };
                    Some(PanelModelResult {
                        id: model.id.clone(),
                        model_name: model.name.clone(),
                        status,
                        first_token_ms: state.first_token_ms,
                        total_ms: state.duration_ms,
                    })
                })
                .collect::<Vec<_>>();
            (!models.is_empty()).then(|| PanelSiteGroup {
                site_id: site.id.clone(),
                site_name: site.name.clone(),
                models,
            })
        })
        .collect::<Vec<_>>();
    let count = |ids: &[String]| {
        ids.iter()
            .filter(|id| model_ids.contains(id.as_str()))
            .count()
    };
    let mut active = PanelProbeProgress {
        completed_count: 0,
        total_count: 0,
    };
    for task in activity.active.values() {
        if count(&task.model_ids) != 0 {
            active.total_count += task.model_ids.len();
            active.completed_count += task.completed_ids.len();
        }
    }
    // 重开进程后只有持久化的单模型完成事实，不能杜撰上次全局批次的数量。
    let persisted = model_ids
        .iter()
        .filter_map(|id| {
            if let Some(task) = activity.active.values().find(|task| {
                task.model_ids
                    .iter()
                    .any(|model_id| model_id.as_str() == *id)
            }) {
                task.previous_finished_at_ms.get(*id).copied()
            } else {
                states.get(*id).map(|state| state.finished_at_ms)
            }
        })
        .filter(|ms| *ms != 0)
        .max()
        .map(|ms| (ms, 1, 1));
    let latest = persisted
        .into_iter()
        .chain(activity.recent.values().filter_map(|completion| {
            let completed = count(&completion.progress.completed_ids);
            (completed != 0).then_some((
                completion.completed_at_ms,
                completion.progress.completed_ids.len(),
                completion.progress.model_ids.len(),
            ))
        }))
        // 同一毫秒结束时优先保留真实批次，不用单模型回退覆盖它。
        .max_by_key(|(ms, _, _)| *ms);
    let last_probe = latest
        .map(|(ms, completed_count, total_count)| {
            Ok::<_, PanelError>(PanelProbeCompletion {
                completed_at: timestamp(ms)?,
                completed_count,
                total_count,
            })
        })
        .transpose()?;
    let next = automatic
        .then(|| {
            eligible
                .iter()
                .flat_map(|site| {
                    site.models
                        .iter()
                        .filter(|model| model.enabled)
                        .map(|model| {
                            let last = states.get(&model.id).map_or(model.created_at_ms, |state| {
                                state.finished_at_ms.max(model.created_at_ms)
                            });
                            next_due_at(last, site.connection.interval_seconds)
                        })
                })
                .min()
        })
        .flatten();
    let schedule = match next {
        Some(ms) => PanelSchedule::Scheduled {
            next_probe_at: timestamp(ms)?,
        },
        None => PanelSchedule::Disabled,
    };
    Ok(PanelSnapshot {
        revision,
        theme,
        state: if groups.is_empty() {
            PanelState::Waiting
        } else {
            PanelState::Ready
        },
        site_count: eligible.len(),
        model_count: model_ids.len(),
        sites: groups,
        last_probe,
        active_probe: (active.total_count != 0).then_some(active),
        schedule,
    })
}
