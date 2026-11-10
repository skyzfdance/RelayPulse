//! 唯一后台调度所有者。宿主在 Ready 后启动，退出先取消再等待事务和任务回收。
use crate::modules::monitoring::Monitoring;
use futures_util::FutureExt;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
pub(crate) struct Lifecycle {
    monitoring: Arc<Monitoring>,
    task: Mutex<Option<JoinHandle<()>>>,
    iq_test: Option<Arc<crate::modules::iq_test::IqTest>>,
}
impl Lifecycle {
    pub(crate) fn start(
        monitoring: Arc<Monitoring>,
    ) -> Result<Self, crate::modules::sites::TaskError> {
        monitoring.claim_scheduler()?;
        let work = monitoring.clone();
        let task = tokio::spawn(async move {
            if std::panic::AssertUnwindSafe(work.clone().schedule())
                .catch_unwind()
                .await
                .is_err()
            {
                work.scheduler_failed();
            }
        });
        Ok(Self {
            task: Mutex::new(Some(task)),
            monitoring,
            iq_test: None,
        })
    }
    pub(crate) fn with_iq_test(mut self, iq_test: Arc<crate::modules::iq_test::IqTest>) -> Self {
        self.iq_test = Some(iq_test);
        self
    }
    pub(crate) async fn stop(&self) {
        if let Some(iq_test) = &self.iq_test {
            iq_test.close();
        }
        self.monitoring.close();
        let task = self.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(task) = task {
            if task.await.is_err() {
                self.monitoring.scheduler_failed();
            }
        }
        self.monitoring.drained().await;
        if let Some(iq_test) = &self.iq_test {
            iq_test.stop().await;
        }
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        if let Some(iq_test) = &self.iq_test {
            iq_test.close();
        }
        self.monitoring.close();
        if let Some(task) = self
            .task
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            task.abort();
        }
    }
}
