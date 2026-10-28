use tokio::sync::watch;
/// 只传播取消，不管理业务任务或创建后台 worker。
#[derive(Clone)]
pub(crate) struct Cancellation(watch::Sender<bool>);
impl Default for Cancellation {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}
impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.0.send_replace(true);
    }
    pub(crate) fn subscribe(&self) -> watch::Receiver<bool> {
        self.0.subscribe()
    }
    pub(crate) async fn cancelled(&self) {
        let mut receiver = self.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }
}
