//! 将已提交的正式健康变化交给通知用例；不接收前端广播或历史快照。
use crate::modules::{
    monitoring::{HealthChange, HealthObserver, HealthTransition},
    notifications::{NotificationContext, NotificationPlatform, NotificationService},
};
use std::sync::Arc;

pub(crate) struct HealthNotifications {
    service: Arc<NotificationService>,
    platform: Arc<dyn NotificationPlatform>,
}

impl HealthNotifications {
    pub(crate) fn new(
        service: Arc<NotificationService>,
        platform: Arc<dyn NotificationPlatform>,
    ) -> Self {
        Self { service, platform }
    }
}

impl HealthObserver for HealthNotifications {
    fn changed(&self, change: &HealthChange) {
        let rule = match change.transition {
            HealthTransition::Failed => "probe.model_failed",
            HealthTransition::Recovered => "probe.model_recovered",
        };
        // 服务记录具体错误分类；通知失败不影响正式记录和健康状态。
        let _ = self.service.send_rule(
            rule,
            &NotificationContext {
                model: &change.model,
                endpoint: &change.endpoint,
                finished_at_ms: change.finished_at_ms,
            },
            self.platform.as_ref(),
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/notifications/health_changes.rs"]
mod tests;
