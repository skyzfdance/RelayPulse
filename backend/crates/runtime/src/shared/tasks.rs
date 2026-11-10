//! spawn_blocking 的工作不会随等待 future 被丢弃而停止，必须由业务所有者排空。
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub struct BlockingActivity {
    count: Mutex<usize>,
    change: watch::Sender<usize>,
}
impl Default for BlockingActivity {
    fn default() -> Self {
        Self {
            count: Mutex::new(0),
            change: watch::channel(0).0,
        }
    }
}
pub struct BlockingGuard(Arc<BlockingActivity>);
impl BlockingActivity {
    pub fn enter(self: &Arc<Self>) -> BlockingGuard {
        let mut count = self.count.lock().unwrap_or_else(|e| e.into_inner());
        *count += 1;
        self.change.send_replace(*count);
        BlockingGuard(self.clone())
    }
    pub async fn drained(&self) {
        let mut count = self.change.subscribe();
        while *count.borrow_and_update() != 0 {
            if count.changed().await.is_err() {
                break;
            }
        }
    }
}
impl Drop for BlockingGuard {
    fn drop(&mut self) {
        let mut count = self.0.count.lock().unwrap_or_else(|e| e.into_inner());
        *count -= 1;
        self.0.change.send_replace(*count);
    }
}

/// 只传播取消，不管理业务任务或创建后台 worker。
#[derive(Clone)]
pub struct Cancellation(watch::Sender<bool>);
impl Default for Cancellation {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl Cancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.0.subscribe()
    }
    pub async fn cancelled(&self) {
        let mut receiver = self.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }
}
