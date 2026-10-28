//! 正式健康状态的边沿变化；不扫描历史、不补发启动前事件。
use super::{Health, HealthState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthTransition {
    Failed,
    Recovered,
}

pub(crate) struct HealthChange {
    pub transition: HealthTransition,
    pub model: String,
    pub endpoint: String,
    pub finished_at_ms: u64,
}

impl HealthChange {
    pub(super) fn between(
        previous: &HealthState,
        next: &HealthState,
        model: &str,
        base_url: &str,
    ) -> Option<Self> {
        let transition = match (previous.health, next.health) {
            (Health::Error, Health::Healthy | Health::Warning) => HealthTransition::Recovered,
            (before, Health::Error) if before != Health::Error => HealthTransition::Failed,
            _ => return None,
        };
        // 通知只携带主机，不把 URL 路径、查询参数或凭据送入系统通知中心。
        let endpoint = reqwest::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "—".into());
        Some(Self {
            transition,
            model: model.into(),
            endpoint,
            finished_at_ms: next.finished_at_ms,
        })
    }
}

/// 在事务提交后的阻塞 worker 中调用；适配方记录发送错误，不回滚探测。
pub(crate) trait HealthObserver: Send + Sync {
    fn changed(&self, change: &HealthChange);
}
