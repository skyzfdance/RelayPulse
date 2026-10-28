//! 组合查询的进程内版本边界；不保存业务数据，也不依赖宿主事件。
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StateVersion {
    pub revision: u64,
    pub writers: usize,
}

pub(crate) struct StateChanges(watch::Sender<StateVersion>);

impl Default for StateChanges {
    fn default() -> Self {
        Self(watch::channel(StateVersion::default()).0)
    }
}

impl StateChanges {
    pub(crate) fn subscribe(&self) -> watch::Receiver<StateVersion> {
        self.0.subscribe()
    }

    pub(crate) fn version(&self) -> StateVersion {
        *self.0.borrow()
    }

    /// 必须在写入之前取得，并随真正的 worker 保留至提交/回滚之后。
    pub(crate) fn begin(self: &Arc<Self>) -> StateWrite {
        self.0.send_modify(|version| {
            version.revision += 1;
            version.writers += 1;
        });
        StateWrite {
            changes: self.clone(),
        }
    }
}

pub(crate) struct StateWrite {
    changes: Arc<StateChanges>,
}

impl Drop for StateWrite {
    fn drop(&mut self) {
        self.changes.0.send_modify(|version| {
            version.writers -= 1;
            version.revision += 1;
        });
    }
}

#[cfg(test)]
#[path = "../../tests/unit/state_changes.rs"]
mod tests;
