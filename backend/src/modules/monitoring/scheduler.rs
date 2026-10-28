//! 调度属于正式监测，后台根任务由 application/lifecycle 持有。
use super::Monitoring;
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{collections::HashSet, sync::Arc, time::Duration};
impl Monitoring {
    pub(crate) async fn schedule(self: Arc<Self>) {
        let cancellation = self.cancellation();
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(60));
        cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut jobs = FuturesUnordered::new();
        let mut cleanups = FuturesUnordered::new();
        let mut active = HashSet::new();
        let mut settings_changes = self.settings_changes();
        let mut automatic_enabled = None;
        loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => break,
                _ = settings_changes.changed() => {
                    // 设置保存只改变后续派发；jobs 中已开始的请求继续执行。
                    automatic_enabled = None;
                },
                Some(()) = cleanups.next(), if !cleanups.is_empty() => {},
                _ = cleanup_interval.tick(), if cleanups.is_empty() => {
                    // 清理与网络任务共同推进，慢磁盘不会停止轮询所有正在探测的站点。
                    cleanups.push(self.cleanup_records());
                },
                Some(id) = jobs.next(), if !jobs.is_empty() => { active.remove(&id); },
                _ = interval.tick() => {
                    if automatic_enabled.is_none() {
                        automatic_enabled = tokio::select! {
                            biased;
                            _ = cancellation.cancelled() => break,
                            enabled = self.automatic_enabled() => enabled.ok(),
                        };
                    }
                    if automatic_enabled != Some(true) { continue; }
                    let due = tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => break,
                        due = self.due_sites() => due,
                    };
                    for id in due {
                        if jobs.len() >= 8 { break; }
                        if !active.insert(id.clone()) { continue; }
                        let service = self.clone();
                        jobs.push(async move { service.run_scheduled(id.clone()).await; id });
                    }
                }
            }
        }
        drop(jobs);
        drop(cleanups);
        self.drained().await;
    }
}
