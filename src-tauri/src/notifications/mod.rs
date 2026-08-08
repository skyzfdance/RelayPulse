mod dispatcher;
mod system;

use std::{collections::HashSet, sync::Arc};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{Clock, SystemClock, TimestampMs},
    persistence::{DatabaseError, Repository},
};

pub use dispatcher::start_dispatcher;
pub(crate) use system::system_dispatch;
pub use system::{
    request_system_notification_permission, send_test_system_notification, system_channel_snapshot,
};

pub const SYSTEM_CHANNEL_ID: &str = "system";
const MAX_DISPATCH_ATTEMPTS: u32 = 3;
const TERMINAL_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPermission {
    Unknown,
    NotRequested,
    Denied,
    Granted,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventTone {
    Critical,
    Positive,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelSnapshot {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub permission: NotificationPermission,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationEventCatalogItem {
    pub event_type: String,
    pub payload_version: u16,
    pub category: String,
    pub category_label: Option<String>,
    pub display_name: String,
    pub description: String,
    pub default_enabled: bool,
    pub supported_channels: Vec<String>,
    pub tone: Option<NotificationEventTone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRule {
    pub event_type: String,
    pub enabled: bool,
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsSnapshot {
    pub revision: u64,
    pub notifications_enabled: bool,
    pub channels: Vec<NotificationChannelSnapshot>,
    pub event_catalog: Vec<NotificationEventCatalogItem>,
    pub rules: Vec<NotificationRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsUpdate {
    pub revision: u64,
    pub notifications_enabled: bool,
    pub rules: Vec<NotificationRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSettingsState {
    pub revision: u64,
    pub notifications_enabled: bool,
    pub rules: Vec<NotificationRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSettingsUpdateOutcome {
    pub settings: NotificationSettingsState,
    pub app_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedNotificationEvent {
    pub(crate) id: String,
    pub(crate) event_type: String,
    pub(crate) payload_version: u16,
    pub(crate) occurred_at_ms: i64,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) dedupe_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFailureReason {
    Authentication,
    RateLimited,
    Timeout,
    Network,
    Upstream,
    Protocol,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRecoveryState {
    Healthy,
    Slow,
}

impl PreparedNotificationEvent {
    pub fn model_failed(
        id: impl Into<String>,
        occurred_at: TimestampMs,
        monitoring_session_id: &str,
        model_id: &str,
        site_name: &str,
        model_name: &str,
        reason: ModelFailureReason,
    ) -> Result<Self, NotificationError> {
        validate_event_identifier(monitoring_session_id)?;
        validate_event_identifier(model_id)?;
        validate_display_name(site_name)?;
        validate_display_name(model_name)?;
        let summary = match reason {
            ModelFailureReason::Authentication => "认证失败",
            ModelFailureReason::RateLimited => "请求受到速率限制",
            ModelFailureReason::Timeout => "探测请求超时",
            ModelFailureReason::Network => "网络连接失败",
            ModelFailureReason::Upstream => "上游服务返回错误",
            ModelFailureReason::Protocol => "响应协议不受支持",
            ModelFailureReason::Unknown => "探测未成功完成",
        };
        Ok(Self {
            id: id.into(),
            event_type: "probe.model_failed".to_owned(),
            payload_version: 1,
            occurred_at_ms: occurred_at.as_i64(),
            title: "模型探测故障".to_owned(),
            body: format!("{site_name} · {model_name}：{summary}"),
            dedupe_key: format!("probe.model_failed:{monitoring_session_id}:{model_id}"),
        })
    }

    pub fn model_recovered(
        id: impl Into<String>,
        occurred_at: TimestampMs,
        monitoring_session_id: &str,
        model_id: &str,
        site_name: &str,
        model_name: &str,
        state: ModelRecoveryState,
    ) -> Result<Self, NotificationError> {
        validate_event_identifier(monitoring_session_id)?;
        validate_event_identifier(model_id)?;
        validate_display_name(site_name)?;
        validate_display_name(model_name)?;
        let summary = match state {
            ModelRecoveryState::Healthy => "已恢复为正常",
            ModelRecoveryState::Slow => "已恢复但响应偏慢",
        };
        Ok(Self {
            id: id.into(),
            event_type: "probe.model_recovered".to_owned(),
            payload_version: 1,
            occurred_at_ms: occurred_at.as_i64(),
            title: "模型探测恢复".to_owned(),
            body: format!("{site_name} · {model_name}：{summary}"),
            dedupe_key: format!("probe.model_recovered:{monitoring_session_id}:{model_id}"),
        })
    }

    #[cfg(test)]
    fn fixture(id: &str, dedupe_key: &str) -> Self {
        Self {
            id: id.to_owned(),
            event_type: "fixture.task_completed".to_owned(),
            payload_version: 1,
            occurred_at_ms: 1_700_000_000_000,
            title: "RelayPulse fixture".to_owned(),
            body: "A deterministic notification fixture completed.".to_owned(),
            dedupe_key: dedupe_key.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotificationDelivery {
    pub event_id: String,
    pub event_type: String,
    pub channel_id: String,
    pub title: String,
    pub body: String,
    pub attempt_count: u32,
    pub lease_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchFailure {
    pub code: &'static str,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReport {
    pub event_id: String,
    pub event_type: String,
    pub channel_id: String,
    pub status: &'static str,
    pub attempt_count: u32,
    pub error_code: Option<&'static str>,
}

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("notification settings changed concurrently")]
    SettingsConflict,
    #[error("notification catalog changed")]
    CatalogChanged,
    #[error("notification settings are invalid")]
    InvalidSettings { field: &'static str },
    #[error("notification settings are unavailable")]
    SettingsUnavailable,
    #[error("notification service is disabled")]
    ServiceDisabled,
    #[error("notification channel was not found")]
    ChannelNotFound,
    #[error("notification channel is unsupported")]
    ChannelUnsupported,
    #[error("notification permission state is unknown")]
    PermissionUnknown,
    #[error("notification permission has not been requested")]
    PermissionNotRequested,
    #[error("notification permission was denied")]
    PermissionDenied,
    #[error("notification dispatch failed")]
    DispatchFailed,
    #[error("notification dispatch failed temporarily")]
    DispatchRetryable,
}

impl From<DatabaseError> for NotificationError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::Conflict {
                entity: "notification-settings",
            } => Self::SettingsConflict,
            DatabaseError::CatalogChanged => Self::CatalogChanged,
            DatabaseError::InvalidInput { field } => Self::InvalidSettings { field },
            _ => Self::SettingsUnavailable,
        }
    }
}

#[derive(Clone)]
pub struct NotificationService {
    repository: Repository,
    clock: Arc<dyn Clock>,
}

impl NotificationService {
    pub fn new(repository: Repository) -> Self {
        Self {
            repository,
            clock: Arc::new(SystemClock),
        }
    }

    #[cfg(test)]
    fn with_clock(repository: Repository, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    pub async fn settings(
        &self,
        system_channel: NotificationChannelSnapshot,
    ) -> Result<NotificationSettingsSnapshot, NotificationError> {
        let event_catalog = event_catalog();
        let settings = self
            .repository
            .notification_settings(event_catalog.clone())
            .await?;
        Ok(snapshot(settings, system_channel, event_catalog))
    }

    pub async fn update_settings(
        &self,
        update: NotificationSettingsUpdate,
        system_channel: NotificationChannelSnapshot,
    ) -> Result<(NotificationSettingsSnapshot, u64), NotificationError> {
        validate_settings_update(&update)?;
        let event_catalog = event_catalog();
        let outcome = self
            .repository
            .update_notification_settings(update, event_catalog.clone(), self.clock.now())
            .await?;
        Ok((
            snapshot(outcome.settings, system_channel, event_catalog),
            outcome.app_revision,
        ))
    }

    pub async fn publish(
        &self,
        event: PreparedNotificationEvent,
    ) -> Result<bool, NotificationError> {
        validate_prepared_event(&event)?;
        Ok(self
            .repository
            .enqueue_notification(event, event_catalog(), self.clock.now())
            .await?)
    }

    pub(crate) async fn dispatch_next<F>(
        &self,
        dispatch: F,
    ) -> Result<Option<DispatchReport>, NotificationError>
    where
        F: FnOnce(&NotificationDelivery) -> Result<(), DispatchFailure>,
    {
        let now = self.clock.now();
        let lease_token = format!("lease_{}", uuid::Uuid::new_v4().simple());
        let Some(delivery) = self.repository.claim_notification(now, lease_token).await? else {
            return Ok(None);
        };

        if !self
            .repository
            .notification_policy_allows(&delivery, event_catalog())
            .await?
        {
            self.repository
                .finish_notification(&delivery, "suppressed", None, None, now)
                .await?;
            return Ok(Some(report(&delivery, "suppressed", None)));
        }

        match dispatch(&delivery) {
            Ok(()) => {
                self.repository
                    .finish_notification(&delivery, "dispatched", None, None, now)
                    .await?;
                Ok(Some(report(&delivery, "dispatched", None)))
            }
            Err(error) if error.retryable && delivery.attempt_count < MAX_DISPATCH_ATTEMPTS => {
                let retry_at = TimestampMs::new(
                    now.as_i64()
                        .saturating_add(retry_delay_ms(delivery.attempt_count)),
                )
                .map_err(|_| NotificationError::SettingsUnavailable)?;
                self.repository
                    .finish_notification(
                        &delivery,
                        "pending",
                        Some(error.code),
                        Some(retry_at),
                        now,
                    )
                    .await?;
                Ok(Some(report(&delivery, "pending", Some(error.code))))
            }
            Err(error) => {
                self.repository
                    .finish_notification(&delivery, "failed", Some(error.code), None, now)
                    .await?;
                Ok(Some(report(&delivery, "failed", Some(error.code))))
            }
        }
    }

    pub(crate) async fn cleanup_terminal_notifications(&self) -> Result<usize, NotificationError> {
        let terminal_before = TimestampMs::new(
            self.clock
                .now()
                .as_i64()
                .saturating_sub(TERMINAL_RETENTION_MS),
        )
        .map_err(|_| NotificationError::SettingsUnavailable)?;
        Ok(self
            .repository
            .cleanup_terminal_notifications(terminal_before)
            .await?)
    }
}

fn snapshot(
    settings: NotificationSettingsState,
    system_channel: NotificationChannelSnapshot,
    event_catalog: Vec<NotificationEventCatalogItem>,
) -> NotificationSettingsSnapshot {
    NotificationSettingsSnapshot {
        revision: settings.revision,
        notifications_enabled: settings.notifications_enabled,
        channels: vec![system_channel],
        event_catalog,
        rules: settings.rules,
    }
}

fn validate_settings_update(update: &NotificationSettingsUpdate) -> Result<(), NotificationError> {
    let catalog = event_catalog();
    if update.rules.len() != catalog.len() {
        return Err(NotificationError::CatalogChanged);
    }
    let mut event_types = HashSet::new();
    for rule in &update.rules {
        let Some(event) = catalog
            .iter()
            .find(|item| item.event_type == rule.event_type)
        else {
            return Err(NotificationError::CatalogChanged);
        };
        if !event_types.insert(rule.event_type.as_str()) {
            return Err(NotificationError::InvalidSettings { field: "rules" });
        }
        let channels = rule.channel_ids.iter().collect::<HashSet<_>>();
        if channels.len() != rule.channel_ids.len()
            || rule.channel_ids.is_empty()
            || rule
                .channel_ids
                .iter()
                .any(|channel| !event.supported_channels.contains(channel))
        {
            return Err(NotificationError::InvalidSettings {
                field: "channel_ids",
            });
        }
    }
    Ok(())
}

fn validate_prepared_event(event: &PreparedNotificationEvent) -> Result<(), NotificationError> {
    let Some(catalog) = event_catalog()
        .into_iter()
        .find(|item| item.event_type == event.event_type)
    else {
        return Err(NotificationError::CatalogChanged);
    };
    if catalog.payload_version != event.payload_version {
        return Err(NotificationError::CatalogChanged);
    }
    if validate_event_identifier(&event.id).is_err()
        || event.occurred_at_ms < 0
        || event.dedupe_key.is_empty()
        || event.dedupe_key.len() > 200
        || event.title.is_empty()
        || event.title.chars().count() > 120
        || event.body.is_empty()
        || event.body.chars().count() > 500
    {
        return Err(NotificationError::InvalidSettings { field: "event" });
    }
    Ok(())
}

fn validate_event_identifier(value: &str) -> Result<(), NotificationError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(NotificationError::InvalidSettings { field: "event" });
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<(), NotificationError> {
    if value.trim().is_empty()
        || value.chars().count() > 120
        || value.chars().any(|character| character.is_control())
    {
        return Err(NotificationError::InvalidSettings { field: "event" });
    }
    Ok(())
}

pub fn event_catalog() -> Vec<NotificationEventCatalogItem> {
    let catalog = vec![
        NotificationEventCatalogItem {
            event_type: "probe.model_failed".to_owned(),
            payload_version: 1,
            category: "model-health".to_owned(),
            category_label: Some("模型健康".to_owned()),
            display_name: "模型探测故障".to_owned(),
            description: "模型达到稳定故障判定时发送系统通知".to_owned(),
            default_enabled: true,
            supported_channels: vec![SYSTEM_CHANNEL_ID.to_owned()],
            tone: Some(NotificationEventTone::Critical),
        },
        NotificationEventCatalogItem {
            event_type: "probe.model_recovered".to_owned(),
            payload_version: 1,
            category: "model-health".to_owned(),
            category_label: Some("模型健康".to_owned()),
            display_name: "模型探测恢复".to_owned(),
            description: "模型从稳定故障恢复时发送系统通知".to_owned(),
            default_enabled: true,
            supported_channels: vec![SYSTEM_CHANNEL_ID.to_owned()],
            tone: Some(NotificationEventTone::Positive),
        },
    ];
    #[cfg(test)]
    {
        let mut catalog = catalog;
        catalog.push(NotificationEventCatalogItem {
            event_type: "fixture.task_completed".to_owned(),
            payload_version: 1,
            category: "fixture".to_owned(),
            category_label: Some("Fixture".to_owned()),
            display_name: "Fixture completed".to_owned(),
            description: "Deterministic infrastructure test event".to_owned(),
            default_enabled: true,
            supported_channels: vec![SYSTEM_CHANNEL_ID.to_owned()],
            tone: Some(NotificationEventTone::Neutral),
        });
        catalog
    }
    #[cfg(not(test))]
    {
        catalog
    }
}

fn retry_delay_ms(attempt_count: u32) -> i64 {
    match attempt_count {
        0 | 1 => 5_000,
        2 => 30_000,
        _ => 300_000,
    }
}

fn report(
    delivery: &NotificationDelivery,
    status: &'static str,
    error_code: Option<&'static str>,
) -> DispatchReport {
    DispatchReport {
        event_id: delivery.event_id.clone(),
        event_type: delivery.event_type.clone(),
        channel_id: delivery.channel_id.clone(),
        status,
        attempt_count: delivery.attempt_count,
        error_code,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{diagnostics::Diagnostics, domain::FixedClock, persistence::Database};

    fn service() -> NotificationService {
        let database = Database::open_in_memory().expect("database opens");
        let repository = Repository::new(database);
        let now = TimestampMs::new(1_700_000_000_000).expect("fixture time is valid");
        NotificationService::with_clock(repository, Arc::new(FixedClock::new(now)))
    }

    fn channel() -> NotificationChannelSnapshot {
        NotificationChannelSnapshot {
            id: SYSTEM_CHANNEL_ID.to_owned(),
            display_name: "System notifications".to_owned(),
            description: "Fixture channel".to_owned(),
            permission: NotificationPermission::Granted,
            supported: true,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_round_trip_uses_a_scoped_revision() {
        let service = service();
        let initial = service.settings(channel()).await.expect("settings load");
        assert_eq!(initial.revision, 0);
        let update = NotificationSettingsUpdate {
            revision: initial.revision,
            notifications_enabled: false,
            rules: initial.rules.clone(),
        };

        let (saved, app_revision) = service
            .update_settings(update, channel())
            .await
            .expect("settings save");

        assert_eq!(saved.revision, 1);
        assert!(!saved.notifications_enabled);
        assert_eq!(app_revision, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_settings_are_rejected_without_a_partial_write() {
        let service = service();
        let initial = service.settings(channel()).await.expect("settings load");
        let update = NotificationSettingsUpdate {
            revision: 0,
            notifications_enabled: false,
            rules: initial.rules,
        };
        service
            .update_settings(update.clone(), channel())
            .await
            .expect("first save succeeds");

        assert!(matches!(
            service.update_settings(update, channel()).await,
            Err(NotificationError::SettingsConflict)
        ));
        assert_eq!(
            service
                .settings(channel())
                .await
                .expect("settings reload")
                .revision,
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disabled_policy_creates_no_outbox_item() {
        let service = service();
        let initial = service.settings(channel()).await.expect("settings load");
        service
            .update_settings(
                NotificationSettingsUpdate {
                    revision: initial.revision,
                    notifications_enabled: false,
                    rules: initial.rules,
                },
                channel(),
            )
            .await
            .expect("settings save");

        let event = PreparedNotificationEvent::fixture("event_disabled", "dedupe_disabled");
        assert!(!service.publish(event).await.expect("publish is evaluated"));
        assert!(service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("dispatcher runs")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn enabled_event_dispatches_once_and_dedupe_is_idempotent() {
        let service = service();
        let event = PreparedNotificationEvent::fixture("event_once", "dedupe_once");
        assert!(service
            .publish(event.clone())
            .await
            .expect("first publish succeeds"));
        assert!(!service
            .publish(event)
            .await
            .expect("duplicate publish converges"));

        let report = service
            .dispatch_next(|delivery| {
                assert_eq!(delivery.title, "RelayPulse fixture");
                Ok(())
            })
            .await
            .expect("dispatcher runs")
            .expect("one delivery is available");

        assert_eq!(report.status, "dispatched");
        assert_eq!(report.attempt_count, 1);
        assert!(service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("dispatcher runs again")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_event_types_and_payload_versions_are_rejected() {
        let service = service();
        let mut unknown =
            PreparedNotificationEvent::fixture("event_unknown_catalog", "dedupe_unknown_catalog");
        unknown.event_type = "fixture.unknown".to_owned();
        assert!(matches!(
            service.publish(unknown).await,
            Err(NotificationError::CatalogChanged)
        ));

        let mut wrong_version =
            PreparedNotificationEvent::fixture("event_wrong_version", "dedupe_wrong_version");
        wrong_version.payload_version = 2;
        assert!(matches!(
            service.publish(wrong_version).await,
            Err(NotificationError::CatalogChanged)
        ));
        assert!(service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("dispatcher remains empty")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn queued_event_is_suppressed_when_settings_are_disabled_before_dispatch() {
        let service = service();
        assert!(service
            .publish(PreparedNotificationEvent::fixture(
                "event_suppressed",
                "dedupe_suppressed",
            ))
            .await
            .expect("event is queued"));
        let initial = service.settings(channel()).await.expect("settings load");
        service
            .update_settings(
                NotificationSettingsUpdate {
                    revision: initial.revision,
                    notifications_enabled: false,
                    rules: initial.rules,
                },
                channel(),
            )
            .await
            .expect("settings are disabled");

        let report = service
            .dispatch_next(|_| panic!("suppressed delivery must not reach the channel"))
            .await
            .expect("dispatcher runs")
            .expect("queued delivery is resolved");

        assert_eq!(report.status, "suppressed");
        assert_eq!(report.attempt_count, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retryable_failure_is_rescheduled_without_immediate_busy_retry() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_retry",
                "dedupe_retry",
            ))
            .await
            .expect("event is queued");

        let report = service
            .dispatch_next(|_| {
                Err(DispatchFailure {
                    code: "notification_dispatch_retryable",
                    retryable: true,
                })
            })
            .await
            .expect("dispatcher runs")
            .expect("delivery is attempted");

        assert_eq!(report.status, "pending");
        assert_eq!(report.error_code, Some("notification_dispatch_retryable"));
        assert!(service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("dispatcher does not busy retry")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retryable_failure_stops_after_three_attempts() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_retry_limit",
                "dedupe_retry_limit",
            ))
            .await
            .expect("event is queued");
        let failure = || {
            Err(DispatchFailure {
                code: "notification_dispatch_retryable",
                retryable: true,
            })
        };

        let first = service
            .dispatch_next(|_| failure())
            .await
            .expect("first attempt runs")
            .expect("first delivery exists");
        assert_eq!(first.status, "pending");

        let second_service = NotificationService::with_clock(
            service.repository.clone(),
            Arc::new(FixedClock::new(
                TimestampMs::new(1_700_000_005_000).expect("second attempt time is valid"),
            )),
        );
        let second = second_service
            .dispatch_next(|_| failure())
            .await
            .expect("second attempt runs")
            .expect("second delivery exists");
        assert_eq!(second.status, "pending");

        let third_service = NotificationService::with_clock(
            service.repository.clone(),
            Arc::new(FixedClock::new(
                TimestampMs::new(1_700_000_035_000).expect("third attempt time is valid"),
            )),
        );
        let third = third_service
            .dispatch_next(|_| failure())
            .await
            .expect("third attempt runs")
            .expect("third delivery exists");
        assert_eq!(third.status, "failed");
        assert_eq!(third.attempt_count, 3);
        assert!(third_service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("terminal delivery is not retried")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn permanent_failure_is_not_retried() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_permanent_failure",
                "dedupe_permanent_failure",
            ))
            .await
            .expect("event is queued");

        let report = service
            .dispatch_next(|_| {
                Err(DispatchFailure {
                    code: "notification_dispatch_failed",
                    retryable: false,
                })
            })
            .await
            .expect("dispatcher runs")
            .expect("delivery is attempted");

        assert_eq!(report.status, "failed");
        assert_eq!(report.attempt_count, 1);
        assert!(service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("permanent failure is not retried")
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cleanup_removes_only_expired_terminal_events() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_a_terminal_cleanup",
                "dedupe_terminal_cleanup",
            ))
            .await
            .expect("terminal event is queued");
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_b_pending_cleanup",
                "dedupe_pending_cleanup",
            ))
            .await
            .expect("pending event is queued");
        service
            .dispatch_next(|_| Ok(()))
            .await
            .expect("terminal event is dispatched")
            .expect("terminal delivery exists");

        let cleanup_service = NotificationService::with_clock(
            service.repository.clone(),
            Arc::new(FixedClock::new(
                TimestampMs::new(1_700_000_000_000 + TERMINAL_RETENTION_MS + 1)
                    .expect("cleanup time is valid"),
            )),
        );
        assert_eq!(
            cleanup_service
                .cleanup_terminal_notifications()
                .await
                .expect("cleanup succeeds"),
            1
        );
        assert_eq!(
            service
                .repository
                .claim_notification(
                    TimestampMs::new(1_700_000_000_000).expect("fixture time is valid"),
                    "lease_pending_after_cleanup".to_owned(),
                )
                .await
                .expect("pending event can still be claimed")
                .expect("pending event remains")
                .event_id,
            "event_b_pending_cleanup"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn expired_dispatch_lease_is_reclaimed_after_a_crash_window() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_reclaim",
                "dedupe_reclaim",
            ))
            .await
            .expect("event is queued");
        let now = TimestampMs::new(1_700_000_000_000).expect("fixture time is valid");
        let first = service
            .repository
            .claim_notification(now, "lease_first".to_owned())
            .await
            .expect("first claim succeeds")
            .expect("delivery is claimed");
        assert_eq!(first.attempt_count, 1);

        let after_expiry =
            TimestampMs::new(now.as_i64() + 30_001).expect("later fixture time is valid");
        let reclaimed = service
            .repository
            .claim_notification(after_expiry, "lease_second".to_owned())
            .await
            .expect("expired claim is recovered")
            .expect("delivery is reclaimed");

        assert_eq!(reclaimed.event_id, first.event_id);
        assert_eq!(reclaimed.attempt_count, 2);
        assert_ne!(reclaimed.lease_token, first.lease_token);
        assert!(matches!(
            service
                .repository
                .finish_notification(&first, "dispatched", None, None, after_expiry)
                .await,
            Err(DatabaseError::Conflict {
                entity: "notification-outbox"
            })
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_claims_have_only_one_effective_worker() {
        let service = service();
        service
            .publish(PreparedNotificationEvent::fixture(
                "event_concurrent_claim",
                "dedupe_concurrent_claim",
            ))
            .await
            .expect("event is queued");
        let now = TimestampMs::new(1_700_000_000_000).expect("fixture time is valid");
        let repository_a = service.repository.clone();
        let repository_b = service.repository.clone();

        let (claim_a, claim_b) = tokio::join!(
            repository_a.claim_notification(now, "lease_concurrent_a".to_owned()),
            repository_b.claim_notification(now, "lease_concurrent_b".to_owned()),
        );
        let claims = [
            claim_a.expect("first worker completes"),
            claim_b.expect("second worker completes"),
        ];

        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_survive_a_complete_database_reopen() {
        let directory = tempfile::tempdir().expect("temporary directory is created");
        let path = directory.path().join("relaypulse.sqlite3");
        {
            let database = Database::open(&path).expect("database opens");
            let service = NotificationService::new(Repository::new(database));
            let initial = service.settings(channel()).await.expect("settings load");
            service
                .update_settings(
                    NotificationSettingsUpdate {
                        revision: initial.revision,
                        notifications_enabled: false,
                        rules: initial.rules,
                    },
                    channel(),
                )
                .await
                .expect("settings save");
        }

        let reopened = NotificationService::new(Repository::new(
            Database::open(&path).expect("database reopens"),
        ));
        let settings = reopened.settings(channel()).await.expect("settings reload");
        assert_eq!(settings.revision, 1);
        assert!(!settings.notifications_enabled);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dispatch_diagnostics_never_include_notification_content() {
        let directory = tempfile::tempdir().expect("temporary directory is created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        let service = service();
        let marker = "sensitive-marker-must-not-enter-diagnostics";
        let now = TimestampMs::new(1_700_000_000_000).expect("fixture time is valid");
        service
            .publish(
                PreparedNotificationEvent::model_failed(
                    "event_diagnostics",
                    now,
                    "session_diagnostics",
                    "model_diagnostics",
                    marker,
                    "Model A",
                    ModelFailureReason::Timeout,
                )
                .expect("safe typed event is prepared"),
            )
            .await
            .expect("event is queued");
        let report = service
            .dispatch_next(|delivery| {
                assert!(delivery.body.contains(marker));
                Ok(())
            })
            .await
            .expect("dispatcher runs")
            .expect("delivery is dispatched");
        diagnostics.record_notification_dispatch(&report);

        let log = fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("diagnostic log is readable");
        assert!(log.contains("notification-dispatched"));
        assert!(!log.contains(marker));
    }

    #[test]
    fn model_event_formatters_accept_only_bounded_safe_fields() {
        let now = TimestampMs::new(1_700_000_000_000).expect("fixture time is valid");
        let event = PreparedNotificationEvent::model_failed(
            "event_model_failed",
            now,
            "session_a",
            "model_a",
            "站点 A",
            "模型 A",
            ModelFailureReason::Timeout,
        )
        .expect("safe event is prepared");
        assert_eq!(event.title, "模型探测故障");
        assert_eq!(event.body, "站点 A · 模型 A：探测请求超时");
        assert!(PreparedNotificationEvent::model_recovered(
            "event_model_recovered",
            now,
            "bad.session",
            "model_a",
            "站点 A",
            "模型 A",
            ModelRecoveryState::Healthy,
        )
        .is_err());
    }
}
