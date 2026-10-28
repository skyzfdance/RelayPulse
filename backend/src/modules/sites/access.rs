use crate::shared::model_client::Cancellation;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TaskError {
    #[error("该站点正在执行其他任务，请稍后重试。")]
    Busy,
    #[error("应用正在退出，任务已取消。")]
    Closing,
}
#[derive(Default)]
struct State {
    sites: HashMap<String, AccessKind>,
    switches: HashSet<String>,
    global: bool,
    closing: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Exclusive,
    Scheduled,
    Switch,
}
impl State {
    fn count(&self) -> usize {
        self.sites.len() + self.switches.len() + usize::from(self.global)
    }
}
pub(crate) struct SiteAccess {
    state: Mutex<State>,
    closing: Cancellation,
    count: watch::Sender<usize>,
}
impl Default for SiteAccess {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            closing: Cancellation::default(),
            count: watch::channel(0).0,
        }
    }
}
pub(super) struct Lease {
    gate: Arc<SiteAccess>,
    site: Option<String>,
    kind: AccessKind,
}
impl SiteAccess {
    pub(super) fn acquire(self: &Arc<Self>, site: Option<&str>) -> Result<Arc<Lease>, TaskError> {
        self.acquire_kind(site, AccessKind::Exclusive)
    }
    pub(super) fn acquire_scheduled(self: &Arc<Self>, site: &str) -> Result<Arc<Lease>, TaskError> {
        self.acquire_kind(Some(site), AccessKind::Scheduled)
    }
    pub(super) fn acquire_switch(self: &Arc<Self>, site: &str) -> Result<Arc<Lease>, TaskError> {
        self.acquire_kind(Some(site), AccessKind::Switch)
    }
    fn acquire_kind(
        self: &Arc<Self>,
        site: Option<&str>,
        kind: AccessKind,
    ) -> Result<Arc<Lease>, TaskError> {
        let mut state = self.state.lock().map_err(|_| TaskError::Closing)?;
        if state.closing {
            return Err(TaskError::Closing);
        }
        let conflict = site.map_or(
            !state.sites.is_empty() || !state.switches.is_empty(),
            |id| {
                state.switches.contains(id)
                    || state.sites.get(id).is_some_and(|active| {
                        // 自动探测只冻结请求配置；开关可以阻止下一次派发，不取消当前请求。
                        !(kind == AccessKind::Switch && *active == AccessKind::Scheduled)
                    })
            },
        );
        if state.global || conflict {
            return Err(TaskError::Busy);
        }
        if let Some(site) = site {
            if kind == AccessKind::Switch {
                state.switches.insert(site.into());
            } else {
                state.sites.insert(site.into(), kind);
            }
        } else {
            state.global = true;
        }
        self.count.send_replace(state.count());
        Ok(Arc::new(Lease {
            gate: self.clone(),
            site: site.map(str::to_string),
            kind,
        }))
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.closing.clone()
    }
    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closing = true;
        }
        self.closing.cancel();
    }
    pub(crate) async fn drained(&self) {
        let mut count = self.count.subscribe();
        while *count.borrow_and_update() != 0 {
            if count.changed().await.is_err() {
                break;
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            if let Some(site) = &self.site {
                if self.kind == AccessKind::Switch {
                    state.switches.remove(site);
                } else {
                    state.sites.remove(site);
                }
            } else {
                state.global = false;
            }
            self.gate.count.send_replace(state.count());
        }
    }
}

impl Lease {
    /// 阻塞事务独立持有租约，取消等待不提前释放数据保护。
    pub(super) async fn blocking<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, tokio::task::JoinError> {
        let lease = self.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            work()
        })
        .await
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/sites/access.rs"]
mod tests;
