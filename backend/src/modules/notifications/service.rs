use super::{catalog, validation, *};
use crate::shared::diagnostics::{Diagnostics, Recorder};
use std::sync::Arc;

pub(crate) struct NotificationService {
    diagnostics: Recorder,
    store: Box<dyn NotificationStore>,
}

impl NotificationService {
    pub(crate) fn new(store: impl NotificationStore + 'static) -> Self {
        Self {
            store: Box::new(store),
            diagnostics: Recorder::default(),
        }
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Arc<Diagnostics>) -> Self {
        self.diagnostics = Recorder::new(diagnostics);
        self
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
        self.diagnostics.observe("notification_load", || {
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            Ok(settings(&stored, platform.permission()?))
        })
    }

    pub(crate) fn save(
        &self,
        input: NotificationSettings,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationSettings, NotificationError> {
        self.diagnostics.observe("notification_save", || {
            validation::rule_ids(input.rules.iter().map(|r| r.id.as_str()))?;
            // 系统查询失败时不写入，避免“接口失败但设置已保存”。客户端授权值不参与决策。
            let permission = platform.permission()?;
            let saved = self.store.save_settings(&input)?;
            Ok(settings(&saved, permission))
        })
    }

    pub(crate) fn template(&self, id: &str) -> Result<TemplateDetail, NotificationError> {
        self.diagnostics.observe("notification_template_load", || {
            catalog::rule(id)?;
            let stored = self.store.load()?;
            validation::stored(&stored)?;
            detail(&stored, id)
        })
    }

    pub(crate) fn save_template(
        &self,
        id: &str,
        update: TemplateUpdate,
    ) -> Result<TemplateDetail, NotificationError> {
        self.diagnostics.observe("notification_template_save", || {
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
        })
    }

    pub(crate) fn request_permission(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationPermission, NotificationError> {
        self.diagnostics
            .observe("notification_permission", || platform.request_permission())
    }

    pub(crate) fn send_test(
        &self,
        notification: TestNotification,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.diagnostics.observe("notification_test", || {
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
        })
    }

    pub(crate) fn send_rule(
        &self,
        id: &str,
        context: &NotificationContext<'_>,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.diagnostics.observe("notification_probe", || {
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
            platform.send(&title, &body, Some(NotificationAction::OpenProbeRecords))
        })
    }

    pub(crate) fn open_settings(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.diagnostics
            .observe("notification_open_settings", || platform.open_settings())
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
