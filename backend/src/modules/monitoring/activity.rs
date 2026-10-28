//! 正式探测执行状态，跟随实际作用域释放；关闭前端不改变任务生命周期。
use crate::shared::state_changes::StateChanges;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone)]
pub(crate) struct ProbeProgress {
    pub site_id: String,
    pub model_ids: Vec<String>,
    pub completed_ids: Vec<String>,
    pub previous_finished_at_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProbeCompletion {
    pub progress: ProbeProgress,
    pub completed_at_ms: u64,
}

#[derive(Default, Clone)]
pub(crate) struct ProbeActivity {
    pub active: HashMap<u64, ProbeProgress>,
    pub recent: HashMap<String, ProbeCompletion>,
    next_id: u64,
}

pub(super) struct ProbeTask {
    id: u64,
    activity: Arc<Mutex<ProbeActivity>>,
    changes: Arc<StateChanges>,
}

impl ProbeTask {
    pub(super) fn start(
        activity: Arc<Mutex<ProbeActivity>>,
        changes: Arc<StateChanges>,
        site_id: String,
        model_ids: Vec<String>,
        previous_finished_at_ms: HashMap<String, u64>,
    ) -> Arc<Self> {
        let _write = changes.begin();
        let mut state = activity.lock().unwrap_or_else(|error| error.into_inner());
        state.next_id += 1;
        let id = state.next_id;
        state.active.insert(
            id,
            ProbeProgress {
                site_id,
                model_ids,
                completed_ids: vec![],
                previous_finished_at_ms,
            },
        );
        drop(state);
        Arc::new(Self {
            id,
            activity,
            changes,
        })
    }

    /// 自动批次在派发前重读开关；后续启用或刚到期的模型沿用原执行语义。
    pub(super) fn include(&self, model_id: &str) {
        let mut state = self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(task) = state.active.get_mut(&self.id) {
            if !task.model_ids.iter().any(|id| id == model_id) {
                let _write = self.changes.begin();
                task.model_ids.push(model_id.to_string());
            }
        }
    }

    /// 仅在正式记录与健康状态事务提交后调用，包括等待方已被取消的 worker。
    pub(super) fn completed(&self, model_id: String) {
        let _write = self.changes.begin();
        let mut state = self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(task) = state.active.get_mut(&self.id) {
            task.completed_ids.push(model_id);
        }
    }
}

impl Drop for ProbeTask {
    fn drop(&mut self) {
        let _write = self.changes.begin();
        let mut state = self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(progress) = state.active.remove(&self.id) {
            if !progress.completed_ids.is_empty() {
                state.recent.insert(
                    progress.site_id.clone(),
                    ProbeCompletion {
                        progress,
                        completed_at_ms: super::health::now_ms(),
                    },
                );
            }
        }
    }
}
