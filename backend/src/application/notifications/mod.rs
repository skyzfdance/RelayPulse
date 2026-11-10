//! 正式监测与通知用例之间的跨模块协调。
mod health_changes;
mod iq_completion;

pub(crate) use health_changes::HealthNotifications;
pub(crate) use iq_completion::IqCompletionNotifications;

pub(crate) fn catalog() -> crate::modules::notifications::EventCatalog {
    use crate::modules::{iq_test, monitoring, notifications::EventCatalog};
    let rules = iq_test::notification_definitions::RULES
        .iter()
        .chain(monitoring::notification_definitions::RULES)
        .cloned()
        .collect();
    EventCatalog::new(rules, {
        let mut keys = std::collections::HashSet::new();
        let mut v: Vec<_> = monitoring::notification_definitions::VARIABLES
            .iter()
            .chain(iq_test::notification_definitions::VARIABLES)
            .filter(|v| keys.insert(v.key))
            .cloned()
            .collect();
        v.sort_by_key(|x| match x.key {
            "time" => 0,
            "app" => 1,
            "model" => 2,
            _ => 3,
        });
        v
    })
    .expect("valid business notification definitions")
}
pub(crate) fn context<'a>(
    model: &str,
    endpoint: &str,
    finished_at_ms: u64,
    action: crate::modules::notifications::NotificationAction,
) -> crate::modules::notifications::NotificationContext<'a> {
    let timestamp = time::OffsetDateTime::from_unix_timestamp((finished_at_ms / 1000) as i64)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
    crate::modules::notifications::NotificationContext {
        action,
        values: std::collections::BTreeMap::from([
            ("model", model.into()),
            ("endpoint", endpoint.into()),
            ("app", "RelayPulse".into()),
            (
                "time",
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    timestamp.year(),
                    u8::from(timestamp.month()),
                    timestamp.day(),
                    timestamp.hour(),
                    timestamp.minute(),
                    timestamp.second()
                ),
            ),
        ]),
    }
}

pub(crate) mod actions;
pub(crate) mod audit;
