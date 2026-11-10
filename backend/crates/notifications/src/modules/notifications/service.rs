use super::{validation, *};
use crate::shared::diagnostics::{Diagnostics, Recorder};
use std::sync::Arc;

/// Atomic mutation snapshots let callers audit their own use cases without a second read.
pub struct SettingsChange<T> {
    pub before: Option<T>,
    pub after: T,
}
pub struct DeliveryAttempt {
    pub trigger: String,
    pub title: String,
    pub body: String,
    pub result: Result<(), NotificationError>,
}
pub struct NotificationService {
    catalog: EventCatalog,
    diagnostics: Recorder,
    mutation: std::sync::Mutex<()>,
    store: Box<dyn NotificationStore>,
}
impl NotificationService {
    pub fn new(store: impl NotificationStore + 'static, catalog: EventCatalog) -> Self {
        Self {
            store: Box::new(store),
            catalog,
            diagnostics: Recorder::default(),
            mutation: Default::default(),
        }
    }
    pub fn with_diagnostics(mut self, diagnostics: Arc<Diagnostics>) -> Self {
        self.diagnostics = Recorder::new(diagnostics);
        self
    }
    pub fn check_ready(&self) -> Result<(), NotificationError> {
        self.diagnostics.observe("notification_initialize", || {
            validation::stored_catalog(&self.store.load()?, &self.catalog)
        })
    }
    pub fn load(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationSettings, NotificationError> {
        self.diagnostics.observe("notification_load", || {
            let stored = self.store.load()?;
            validation::stored_catalog(&stored, &self.catalog)?;
            Ok(settings(&self.catalog, &stored, platform.permission()?))
        })
    }
    pub fn save(
        &self,
        input: NotificationSettings,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationSettings, NotificationError> {
        self.save_with_snapshot(input, platform)
            .map(|change| change.after)
    }
    pub fn save_with_snapshot(
        &self,
        input: NotificationSettings,
        platform: &dyn NotificationPlatform,
    ) -> Result<SettingsChange<NotificationSettings>, NotificationError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        self.diagnostics.observe("notification_save", || {
            let before = self
                .store
                .load()
                .ok()
                .map(|s| settings(&self.catalog, &s, NotificationPermission::Unknown));
            validation::rule_ids(input.rules.iter().map(|r| r.id.as_str()), &self.catalog)?;
            let permission = platform.permission()?;
            let saved = self.store.save_settings(&input)?;
            Ok(SettingsChange {
                before,
                after: settings(&self.catalog, &saved, permission),
            })
        })
    }
    pub fn template(&self, id: &str) -> Result<TemplateDetail, NotificationError> {
        self.diagnostics.observe("notification_template_load", || {
            self.catalog.rule(id)?;
            let stored = self.store.load()?;
            validation::stored_catalog(&stored, &self.catalog)?;
            detail(&self.catalog, &stored, id)
        })
    }
    pub fn save_template(
        &self,
        id: &str,
        update: TemplateUpdate,
    ) -> Result<TemplateDetail, NotificationError> {
        self.save_template_with_snapshot(id, update)
            .map(|change| change.after)
    }
    pub fn save_template_with_snapshot(
        &self,
        id: &str,
        update: TemplateUpdate,
    ) -> Result<SettingsChange<TemplateDetail>, NotificationError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|e| e.into_inner());
        self.diagnostics.observe("notification_template_save", || {
            let before = self
                .store
                .load()
                .ok()
                .and_then(|s| detail(&self.catalog, &s, id).ok());
            self.catalog.rule(id)?;
            let custom = match update.template_type {
                TemplateMode::Default => None,
                TemplateMode::Custom => {
                    validation::template(&update, &self.catalog)?;
                    Some(&update)
                }
            };
            Ok(SettingsChange {
                before,
                after: detail(&self.catalog, &self.store.save_template(id, custom)?, id)?,
            })
        })
    }
    pub fn request_permission(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<NotificationPermission, NotificationError> {
        self.diagnostics
            .observe("notification_permission", || platform.request_permission())
    }
    pub fn send_test(
        &self,
        notification: TestNotification,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.diagnostics.observe("notification_test", || {
            validation::test_notification(&notification)?;
            let stored = self.store.load()?;
            validation::stored_catalog(&stored, &self.catalog)?;
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
    pub fn variables(&self) -> Vec<NotificationVariable> {
        self.catalog.variables.clone()
    }
    pub fn send_rule(
        &self,
        id: &str,
        context: &NotificationContext<'_>,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.deliver_rule(id, context, platform)?
            .map(|attempt| attempt.result)
            .unwrap_or(Ok(()))
    }
    /// None means disabled or unauthorized. An attempted delivery retains its rendered
    /// product and platform result so each caller can interpret and audit it once.
    pub fn deliver_rule(
        &self,
        id: &str,
        context: &NotificationContext<'_>,
        platform: &dyn NotificationPlatform,
    ) -> Result<Option<DeliveryAttempt>, NotificationError> {
        let prepared = self.diagnostics.observe("notification_prepare", || {
            let definition = self.catalog.rule(id)?;
            let stored = self.store.load()?;
            validation::stored_catalog(&stored, &self.catalog)?;
            let rule = stored
                .rules
                .iter()
                .find(|rule| rule.id == id)
                .cloned()
                .unwrap_or(StoredRule {
                    id: id.into(),
                    status: true,
                    custom: None,
                });
            if !stored.enabled
                || !rule.status
                || platform.permission()? != NotificationPermission::Granted
            {
                return Ok::<_, NotificationError>(None);
            }
            let template = rule.custom.unwrap_or_else(|| definition.default_template());
            validation::template(&template, &self.catalog)?;
            let (title, body) = super::rendering::render(&template, context)?;
            Ok(Some((definition.title.to_owned(), title, body)))
        })?;
        Ok(prepared.map(|(trigger, title, body)| {
            let result = self.diagnostics.observe("notification_send", || {
                platform.send(&title, &body, Some(context.action.clone()))
            });
            DeliveryAttempt {
                trigger,
                title,
                body,
                result,
            }
        }))
    }
    pub fn open_settings(
        &self,
        platform: &dyn NotificationPlatform,
    ) -> Result<(), NotificationError> {
        self.diagnostics
            .observe("notification_open_settings", || platform.open_settings())
    }
}

fn summary(catalog: &EventCatalog, stored: &StoredRule) -> RuleSummary {
    // 调用方已经校验固定规则集合。
    let rule = catalog
        .rules
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
    catalog: &EventCatalog,
    stored: &StoredSettings,
    authorization: NotificationPermission,
) -> NotificationSettings {
    NotificationSettings {
        notifications_enabled: stored.enabled,
        authorization,
        rules: catalog
            .rules
            .iter()
            .map(|r| {
                let stored = stored
                    .rules
                    .iter()
                    .find(|s| s.id == r.id)
                    .cloned()
                    .unwrap_or(StoredRule {
                        id: r.id.into(),
                        status: true,
                        custom: None,
                    });
                summary(catalog, &stored)
            })
            .collect(),
    }
}

fn detail(
    catalog: &EventCatalog,
    stored: &StoredSettings,
    id: &str,
) -> Result<TemplateDetail, NotificationError> {
    let rule = catalog.rule(id)?;
    let stored = stored
        .rules
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .unwrap_or(StoredRule {
            id: id.into(),
            status: true,
            custom: None,
        });
    let template = stored
        .custom
        .clone()
        .unwrap_or_else(|| rule.default_template());
    Ok(TemplateDetail {
        summary: summary(catalog, &stored),
        marker_id: template.marker_id,
        title_template: template.title_template,
        body_template: template.body_template,
        default_marker_id: rule.marker.into(),
        default_title_template: rule.title_template.into(),
        default_body_template: rule.body_template.into(),
    })
}
