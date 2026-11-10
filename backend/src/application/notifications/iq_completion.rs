use crate::modules::{
    iq_test::{CompletionObserver, IqRun},
    notifications::{NotificationPlatform, NotificationService},
};
use std::sync::Arc;

pub(crate) struct IqCompletionNotifications {
    service: Arc<NotificationService>,
    platform: Arc<dyn NotificationPlatform>,
    logs: Arc<crate::modules::operation_logs::OperationLogs>,
}
impl IqCompletionNotifications {
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
impl CompletionObserver for IqCompletionNotifications {
    fn completed(&self, run: &IqRun) {
        let _ = super::audit::send_rule(
            &self.service,
            &self.logs,
            crate::modules::operation_logs::OperationSource::User,
            "iq.run_completed",
            &super::context(
                "IQ 测试",
                "",
                run.finished_at_ms.unwrap_or(run.started_at_ms),
                crate::modules::notifications::NotificationAction {
                    target: "iq_run".into(),
                    parameters: std::collections::BTreeMap::from([(
                        "runId".into(),
                        run.id.clone(),
                    )]),
                },
            ),
            self.platform.as_ref(),
        );
    }
}
