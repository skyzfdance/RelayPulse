use super::{catalog, validation, *};
use crate::modules::operation_logs::*;
use crate::shared::diagnostics::{Diagnostics, Recorder};
use std::sync::Arc;

/// 只接受宿主边界的固定分类，不接收前端文本或 CommandError。
pub(crate) enum NotificationBoundaryFailure {
    InvalidRequest,
    PlatformUnavailable,
    TaskFailed,
}

pub(crate) struct NotificationService {
    diagnostics: Recorder,
    operation_logs: OperationRecorder,
    mutation: std::sync::Mutex<()>,
    store: Box<dyn NotificationStore>,
}

impl NotificationService {
    pub(crate) fn new(store: impl NotificationStore + 'static) -> Self {
        Self {
            store: Box::new(store),
            diagnostics: Recorder::default(),
            operation_logs: Default::default(),
            mutation: Default::default(),
        }
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Arc<Diagnostics>) -> Self {
        self.diagnostics = Recorder::new(diagnostics);
        self
    }

    pub(crate) fn with_operation_logs(mut self, logs: Arc<OperationLogs>) -> Self {
        self.operation_logs = OperationRecorder::new(logs);
        self
    }
    pub(crate) fn record_boundary_failure(
        &self,
        action: OperationAction,
        failure: NotificationBoundaryFailure,
    ) {
        let reason = match failure {
            NotificationBoundaryFailure::InvalidRequest => {
                "通知请求字段缺失、类型错误或含未知字段。"
            }
            NotificationBoundaryFailure::PlatformUnavailable => "系统通知平台尚未就绪。",
            NotificationBoundaryFailure::TaskFailed => "系统通知任务未能完成。",
        };
        self.operation_logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            OperationResult::Failure,
            "通知设置",
            "通知操作未完成",
            OperationLogContent::Failure {
                activity: action.label().into(),
                target: "通知设置".into(),
                reason: reason.into(),
            },
        ));
    }
    fn log_result<T>(
        &self,
        action: OperationAction,
        target: &str,
        content: OperationLogContent,
        result: &Result<T, NotificationError>,
    ) {
        let (outcome, content) = match result {
            Ok(_) => (OperationResult::Success, content),
            Err(error) => (
                notification_result(error),
                OperationLogContent::Failure {
                    activity: "通知操作".into(),
                    target: target.into(),
                    reason: notification_reason(error).into(),
                },
            ),
        };
        self.operation_logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            outcome,
            target,
            if result.is_ok() {
                "已完成通知操作"
            } else {
                "通知操作未完成"
            },
            content,
        ));
    }

    /// 启动仅校验持久化配置；系统授权失败不能伪装成数据库损坏。
    pub(crate) fn check_ready(&self) -> Result<(), NotificationError> {
        self.diagnostics.observe("notification_initialize", || {
            validation::stored(&self.store.load()?)
        })
    }

    pub(crate) fn load(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationSettings, NotificationError> {
        let result = self.diagnostics.observe("notification_load", || {
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            Ok(settings(&stored, platform.permission()?))
        });
        if result.is_err() {
            self.log_result(
                OperationAction::PageLoadFailed,
                "通知设置",
                OperationLogContent::Snapshot { fields: vec![] },
                &result,
            );
        }
        result
    }

    pub(crate) fn save(
        &self,
        input: NotificationSettings,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationSettings, NotificationError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        let before = self
            .store
            .load()
            .ok()
            .map(|s| settings(&s, NotificationPermission::Unknown));
        let result = self.diagnostics.observe("notification_save", || {
            validation::rule_ids(input.rules.iter().map(|r| r.id.as_str()))?;
            // 系统查询失败时不写入，避免“接口失败但设置已保存”。客户端授权值不参与决策。
            let permission = platform.permission()?;
            let saved = self.store.save_settings(&input)?;
            Ok(settings(&saved, permission))
        });
        let fields = result
            .as_ref()
            .map(|after| changes(&before, after))
            .unwrap_or_default();
        self.log_result(
            OperationAction::NotificationSettingsUpdated,
            "通知设置",
            OperationLogContent::Changes { changes: fields },
            &result,
        );
        result
    }

    pub(crate) fn template(&self, id: &str) -> Result<TemplateDetail, NotificationError> {
        let result = self.diagnostics.observe("notification_template_load", || {
            catalog::rule(id)?;
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            detail(&stored, id)
        });
        if result.is_err() {
            self.log_result(
                OperationAction::DetailReadFailed,
                "通知模板",
                OperationLogContent::Snapshot { fields: vec![] },
                &result,
            );
        }
        result
    }

    pub(crate) fn save_template(
        &self,
        id: &str,
        update: TemplateUpdate,
    ) -> Result<TemplateDetail, NotificationError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        let before = self.store.load().ok().and_then(|s| detail(&s, id).ok());
        let result = self.diagnostics.observe("notification_template_save", || {
            catalog::rule(id)?;
            // default 的三个内容字段忽略，生效值由固定目录决定。
            let custom = match update.template_type {
                TemplateMode::Default => None,
                TemplateMode::Custom => {
                    validation::template(&update)?;
                    Some(&update)
                }
            };
            detail(&self.store.save_template(id, custom)?, id)
        });
        let fields = result
            .as_ref()
            .map(|after| changes(&before, after))
            .unwrap_or_default();
        self.log_result(
            OperationAction::NotificationTemplateUpdated,
            "通知模板",
            OperationLogContent::Changes { changes: fields },
            &result,
        );
        result
    }

    pub(crate) fn request_permission(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationPermission, NotificationError> {
        let result = self
            .diagnostics
            .observe("notification_permission", || platform.request_permission());
        let mut event = OperationEvent::new(
            OperationAction::NotificationPermissionRequested,
            OperationSource::User,
            OperationResult::Success,
            "系统通知授权",
            "已请求系统通知授权",
            OperationLogContent::Snapshot {
                fields: snapshot_fields(&result.as_ref().ok()),
            },
        );
        if let Err(error) = &result {
            event.result = notification_result(error);
            event.content = OperationLogContent::Failure {
                activity: "请求授权".into(),
                target: "系统通知".into(),
                reason: notification_reason(error).into(),
            };
        } else if !matches!(result, Ok(NotificationPermission::Granted)) {
            event.result = OperationResult::Blocked;
        }
        self.operation_logs.record_best_effort(event);
        result
    }

    pub(crate) fn send_test(
        &self,
        notification: TestNotification,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        let title = notification.title.clone();
        let body = notification.body.clone();
        let result = self.diagnostics.observe("notification_test", || {
            validation::test_notification(&notification)?;
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            if !stored.enabled {
                return Err(NotificationError::Disabled);
            }
            match platform.permission()? {
                NotificationPermission::Granted => {
                    platform.send(&notification.title, &notification.body, None)
                }
                other => Err(NotificationError::Permission(other)),
            }
        });
        self.record_notification(
            OperationAction::TestNotification,
            OperationSource::User,
            "用户测试通知",
            title,
            body,
            &result,
        );
        result
    }

    pub(crate) fn send_rule(
        &self,
        id: &str,
        context: &NotificationContext<'_>,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        let mut attempted = false;
        let result = self.diagnostics.observe("notification_probe", || {
            let definition = catalog::rule(id)?;
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            let rule = stored
                .rules
                .iter()
                .find(|rule| rule.id == id)
                .ok_or(NotificationError::CorruptSettings)?;
            // 自动发送不弹授权请求；关闭或未授权时直接跳过，不在重启后补发。
            if !stored.enabled || !rule.status {
                return Ok(());
            }
            if platform.permission()? != NotificationPermission::Granted {
                return Ok(());
            }
            let template = rule
                .custom
                .clone()
                .unwrap_or_else(|| definition.default_template());
            let (title, body) = super::rendering::render(&template, context)?;
            attempted = true;
            let result = platform.send(&title, &body, Some(NotificationAction::OpenProbeRecords));
            self.record_notification(
                OperationAction::AutomaticNotification,
                context.source,
                definition.title,
                title,
                body,
                &result,
            );
            result
        });
        if let Some(error) = result.as_ref().err().filter(|_| !attempted) {
            let reason = notification_reason(error);
            self.operation_logs.record_best_effort(OperationEvent::new(
                OperationAction::AutomaticNotification,
                context.source,
                OperationResult::Failure,
                "自动通知",
                reason,
                OperationLogContent::Failure {
                    activity: "自动通知".into(),
                    target: context.model.into(),
                    reason: reason.into(),
                },
            ));
        }
        result
    }

    fn record_notification(
        &self,
        action: OperationAction,
        source: OperationSource,
        trigger: &str,
        title: String,
        body: String,
        result: &Result<(), NotificationError>,
    ) {
        self.operation_logs.record_best_effort(OperationEvent::new(
            action,
            source,
            result
                .as_ref()
                .err()
                .map(notification_result)
                .unwrap_or(OperationResult::Success),
            "系统通知",
            if result.is_ok() {
                "通知已提交给系统"
            } else {
                "通知未发送"
            },
            OperationLogContent::Notification {
                trigger: trigger.into(),
                title,
                body,
                send_result: result
                    .as_ref()
                    .err()
                    .map(notification_reason)
                    .unwrap_or("系统 API 已接受发送请求")
                    .into(),
            },
        ));
    }

    pub(crate) fn open_settings(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        let result = self
            .diagnostics
            .observe("notification_open_settings", || platform.open_settings());
        if result.is_err() {
            self.log_result(
                OperationAction::DetailReadFailed,
                "通知系统设置",
                OperationLogContent::Snapshot { fields: vec![] },
                &result,
            );
        }
        result
    }
}

fn summary(stored: &StoredRule) -> RuleSummary {
    // 调用方已经校验固定规则集合。
    let rule = catalog::RULES
        .iter()
        .find(|r| r.id == stored.id)
        .expect("validated rule");
    RuleSummary {
        id: rule.id.into(),
        title: rule.title.into(),
        sub_title: rule.sub_title.into(),
        status: stored.status,
        channel: "系统通知".into(),
        template_type: if stored.custom.is_some() {
            TemplateMode::Custom
        } else {
            TemplateMode::Default
        },
    }
}

fn settings(
    stored: &StoredSettings,
    authorization: NotificationPermission,
) -> NotificationSettings {
    NotificationSettings {
        notifications_enabled: stored.enabled,
        authorization,
        rules: stored.rules.iter().map(summary).collect(),
    }
}

fn detail(stored: &StoredSettings, id: &str) -> Result<TemplateDetail, NotificationError> {
    let rule = catalog::rule(id)?;
    let stored = stored
        .rules
        .iter()
        .find(|r| r.id == id)
        .ok_or(NotificationError::CorruptSettings)?;
    let template = stored
        .custom
        .clone()
        .unwrap_or_else(|| rule.default_template());
    Ok(TemplateDetail {
        summary: summary(stored),
        marker_id: template.marker_id,
        title_template: template.title_template,
        body_template: template.body_template,
        default_marker_id: rule.marker.into(),
        default_title_template: rule.title_template.into(),
        default_body_template: rule.body_template.into(),
    })
}

fn notification_result(error: &NotificationError) -> OperationResult {
    match error {
        NotificationError::Disabled
        | NotificationError::Permission(_)
        | NotificationError::ApplicationIdentity => OperationResult::Blocked,
        _ => OperationResult::Failure,
    }
}
fn notification_reason(error: &NotificationError) -> &'static str {
    match error {
        NotificationError::Disabled => "通知已禁用，未尝试发送。",
        NotificationError::Permission(_) => "未取得系统通知授权，未尝试发送。",
        NotificationError::ApplicationIdentity => "应用身份不支持系统通知。",
        NotificationError::Platform(_) => "系统通知平台操作失败。",
        NotificationError::Storage(_) | NotificationError::CorruptSettings => {
            "无法读取或保存通知配置。"
        }
        NotificationError::InvalidInput { .. } | NotificationError::RuleNotFound => {
            "通知配置或模板不合法。"
        }
    }
}
