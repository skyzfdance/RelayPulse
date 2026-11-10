//! 将已提交的正式健康变化交给通知用例；不接收前端广播或历史快照。
use crate::modules::{
    monitoring::{HealthChange, HealthObserver, HealthTransition},
    notifications::{NotificationPlatform, NotificationService},
};
use std::sync::Arc;

pub(crate) struct HealthNotifications {
    service: Arc<NotificationService>,
    platform: Arc<dyn NotificationPlatform>,
    logs: Arc<crate::modules::operation_logs::OperationLogs>,
}

impl HealthNotifications {
    pub(crate) fn new(
        service: Arc<NotificationService>,
        platform: Arc<dyn NotificationPlatform>,
        logs: Arc<crate::modules::operation_logs::OperationLogs>,
    ) -> Self {
        Self {
            service,
            platform,
            logs,
        }
    }
}

impl HealthObserver for HealthNotifications {
    fn changed(&self, change: &HealthChange) {
        let rule = match change.transition {
            HealthTransition::Failed => "probe.model_failed",
            HealthTransition::Recovered => "probe.model_recovered",
        };
        // 服务记录具体错误分类；通知失败不影响正式记录和健康状态。
        let _ = super::audit::send_rule(
            &self.service,
            &self.logs,
            change.source,
            rule,
            &super::context(
                &change.model,
                &change.endpoint,
                change.finished_at_ms,
                crate::modules::notifications::NotificationAction {
                    target: "probe_records".into(),
                    parameters: Default::default(),
                },
            ),
            self.platform.as_ref(),
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/notifications/health_changes.rs"]
mod tests;
