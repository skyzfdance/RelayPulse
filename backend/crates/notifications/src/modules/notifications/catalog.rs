use super::{
    NotificationError, NotificationMarker, NotificationVariable, TemplateMode, TemplateUpdate,
};

#[derive(Clone)]
pub struct RuleDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub sub_title: &'static str,
    pub marker: &'static str,
    pub title_template: &'static str,
    pub body_template: &'static str,
}

impl RuleDefinition {
    pub fn default_template(&self) -> TemplateUpdate {
        TemplateUpdate {
            template_type: TemplateMode::Default,
            marker_id: self.marker.into(),
            title_template: self.title_template.into(),
            body_template: self.body_template.into(),
        }
    }
}

pub fn markers() -> Vec<NotificationMarker> {
    [
        ("none", "", "无标记", "neutral"),
        ("info", "ℹ️", "信息", "info"),
        ("success", "✅", "成功", "positive"),
        ("warning", "⚠️", "警告", "warning"),
        ("error", "❌", "错误", "critical"),
        ("bell", "🔔", "提醒", "info"),
        ("fire", "🔥", "严重", "critical"),
    ]
    .into_iter()
    .map(|(marker_id, symbol, name, tone)| NotificationMarker {
        marker_id,
        symbol,
        name,
        tone,
    })
    .collect()
}

#[derive(Clone, Default)]
pub struct EventCatalog {
    pub rules: Vec<RuleDefinition>,
    pub variables: Vec<NotificationVariable>,
}
impl EventCatalog {
    pub fn new(
        rules: Vec<RuleDefinition>,
        variables: Vec<NotificationVariable>,
    ) -> Result<Self, NotificationError> {
        let mut ids = std::collections::HashSet::new();
        for rule in &rules {
            if rule.id.is_empty()
                || rule.id.len() > 200
                || !rule
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                || !ids.insert(rule.id)
            {
                return Err(NotificationError::CorruptSettings);
            }
        }
        let mut keys = std::collections::HashSet::new();
        for variable in &variables {
            if variable.key.is_empty()
                || variable.key.len() > 200
                || !variable
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !keys.insert(variable.key)
            {
                return Err(NotificationError::CorruptSettings);
            }
        }
        let catalog = Self { rules, variables };
        for rule in &catalog.rules {
            super::validation::template(&rule.default_template(), &catalog)?;
        }
        Ok(catalog)
    }
    pub fn rule(&self, id: &str) -> Result<&RuleDefinition, NotificationError> {
        self.rules
            .iter()
            .find(|r| r.id == id)
            .ok_or(NotificationError::RuleNotFound)
    }
    pub fn defaults(&self) -> Vec<super::StoredRule> {
        self.rules
            .iter()
            .map(|r| super::StoredRule {
                id: r.id.into(),
                status: true,
                custom: None,
            })
            .collect()
    }
}
