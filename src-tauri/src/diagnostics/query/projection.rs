use std::collections::{HashMap, HashSet};

use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::types::{
    ConfigurationChangeDto, DataCollectionSource, DiagnosticsDetailsDto, LifecycleEvent,
    OccurredAtPrecision, OccurredAtQualifier, OperationCategory, OperationExceptionDto,
    OperationOccurredAtDto, OperationRecordDetailsDto, OperationRecordSpecificDetailsDto,
    OperationTrigger, ProjectedRecord,
};

const MAX_PENDING_OPERATIONS: usize = 4_096;
const MAX_CONFIGURATION_CHANGES: usize = 16;

pub(super) struct Projector {
    completed_startup_sessions: HashSet<String>,
    pending_legacy_operations: HashMap<String, Value>,
    non_mutating_operations: HashSet<String>,
}

impl Projector {
    pub(super) fn new() -> Self {
        Self {
            completed_startup_sessions: HashSet::new(),
            pending_legacy_operations: HashMap::new(),
            non_mutating_operations: HashSet::new(),
        }
    }

    pub(super) fn project(&mut self, event: Value) -> Option<ProjectedRecord> {
        match event.get("event")?.as_str()? {
            "application-startup-completed" => self.startup_completed(&event),
            "application-started" => self.startup(&event),
            "application-exited" => self.clean_exit(&event),
            "previous-session-terminated-unexpectedly" => self.abnormal_exit(&event),
            "intelligence-ladder-refresh-finished" => self.intelligence_collection_finished(&event),
            "intelligence-ladder-refresh-skipped" => self.intelligence_collection_skipped(&event),
            "diagnostics-exported" => self.diagnostics_export(&event),
            "diagnostic-logs-cleared" => self.diagnostics_cleanup(&event),
            "diagnostic-log-limit-configured" => self.log_limit_change(&event),
            "model-sync-confirmation-required" => self.model_sync_confirmation_required(&event),
            "operation-finished" => self.operation_finished(event),
            "operation-started" => self.legacy_operation_started(&event),
            _ => None,
        }
    }

    fn startup_completed(&mut self, event: &Value) -> Option<ProjectedRecord> {
        if let Some(session_id) = event.get("sessionId").and_then(Value::as_str) {
            self.completed_startup_sessions
                .insert(session_id.to_owned());
        }
        let occurred_at = exact_time(event.get("timestamp")?.as_str()?)?;
        let app_version = event
            .get("applicationVersion")
            .and_then(Value::as_str)
            .filter(|value| safe_runtime_token(value, 32))
            .map(ToOwned::to_owned);
        lifecycle_record(
            occurred_at,
            OperationTrigger::SystemStartup,
            "启动 RelayPulse",
            "RelayPulse 已启动，并恢复可用的本地配置与后台任务。",
            LifecycleEvent::Started,
            LifecycleOptions {
                launch_method: Some("用户打开".to_owned()),
                app_version,
                restored_site_count: event.get("restoredSiteCount").and_then(Value::as_u64),
                restored_model_count: event.get("restoredModelCount").and_then(Value::as_u64),
                background_tasks_started: event
                    .get("backgroundTasksStarted")
                    .and_then(Value::as_bool),
                ..LifecycleOptions::default()
            },
            None,
        )
    }

    fn startup(&self, event: &Value) -> Option<ProjectedRecord> {
        if event
            .get("sessionId")
            .and_then(Value::as_str)
            .is_some_and(|session| self.completed_startup_sessions.contains(session))
        {
            return None;
        }
        let occurred_at = exact_time(event.get("timestamp")?.as_str()?)?;
        let app_version = event
            .get("applicationVersion")
            .and_then(Value::as_str)
            .filter(|value| safe_runtime_token(value, 32))
            .map(ToOwned::to_owned);
        lifecycle_record(
            occurred_at,
            OperationTrigger::SystemStartup,
            "启动 RelayPulse",
            "RelayPulse 已启动。",
            LifecycleEvent::Started,
            LifecycleOptions {
                app_version,
                ..LifecycleOptions::default()
            },
            None,
        )
    }

    fn clean_exit(&self, event: &Value) -> Option<ProjectedRecord> {
        lifecycle_record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationTrigger::SystemShutdown,
            "正常退出 RelayPulse",
            "RelayPulse 已正常退出。",
            LifecycleEvent::Exited,
            LifecycleOptions {
                settings_saved: event.get("settingsSaved").and_then(Value::as_bool),
                window_state_saved: event.get("windowStateSaved").and_then(Value::as_bool),
                background_tasks_stopped: event
                    .get("backgroundTasksStopped")
                    .and_then(Value::as_bool),
                ..LifecycleOptions::default()
            },
            None,
        )
    }

    fn abnormal_exit(&self, event: &Value) -> Option<ProjectedRecord> {
        let detected_at = event.get("timestamp")?.as_str()?;
        let previous_started = event
            .get("previousStartedAt")
            .and_then(Value::as_str)
            .and_then(canonical_time);
        let occurred_at = previous_started.as_deref().map_or_else(
            || approximate_time(detected_at, OccurredAtQualifier::About),
            |value| approximate_time(value, OccurredAtQualifier::After),
        )?;
        lifecycle_record(
            occurred_at,
            OperationTrigger::SystemRecovery,
            "检测到上次异常退出",
            "本次启动时发现上一次会话缺少正常退出标记。",
            LifecycleEvent::AbnormalExit,
            LifecycleOptions {
                previous_session_started_at: previous_started,
                ..LifecycleOptions::default()
            },
            Some(OperationExceptionDto {
                reason: "上一次会话未记录正常退出。".to_owned(),
                safe_code: Some("previous_session_incomplete".to_owned()),
                last_step: None,
                recommended_action: Some("如问题再次发生，请导出诊断包。".to_owned()),
            }),
        )
    }

    fn intelligence_collection_finished(&self, event: &Value) -> Option<ProjectedRecord> {
        let trigger = intelligence_collection_trigger(event.get("trigger")?.as_str()?)?;
        let outcome = event.get("outcome")?.as_str()?;
        let (action_label, description, entry_count, exception) = match outcome {
            "succeeded" => (
                "采集智力天梯数据",
                "RelayPulse 已更新智力天梯数据。",
                Some(event.get("entryCount")?.as_u64()?),
                None,
            ),
            "failed" => {
                let error_code = event.get("errorCode")?.as_str()?;
                (
                    "采集智力天梯数据失败",
                    "RelayPulse 未能更新智力天梯数据。",
                    None,
                    Some(intelligence_collection_exception(error_code)?),
                )
            }
            _ => return None,
        };
        record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationCategory::DataCollection,
            trigger,
            RecordMetadata::new(action_label, description, Some("智力天梯".to_owned())),
            OperationRecordSpecificDetailsDto::DataCollection {
                source: DataCollectionSource::IntelligenceLadder,
                entry_count,
            },
            exception,
        )
    }

    fn intelligence_collection_skipped(&self, event: &Value) -> Option<ProjectedRecord> {
        if event.get("reason")?.as_str()? != "circuit_open" {
            return None;
        }
        record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationCategory::DataCollection,
            intelligence_collection_trigger(event.get("trigger")?.as_str()?)?,
            RecordMetadata::new(
                "采集智力天梯数据失败",
                "RelayPulse 暂时无法采集智力天梯数据。",
                Some("智力天梯".to_owned()),
            ),
            OperationRecordSpecificDetailsDto::DataCollection {
                source: DataCollectionSource::IntelligenceLadder,
                entry_count: None,
            },
            Some(OperationExceptionDto {
                reason: "智力天梯采集服务暂时不可用。".to_owned(),
                safe_code: Some("circuit_open".to_owned()),
                last_step: Some("启动智力天梯数据采集".to_owned()),
                recommended_action: Some("请重新启动 RelayPulse 后重试。".to_owned()),
            }),
        )
    }

    fn diagnostics_export(&self, event: &Value) -> Option<ProjectedRecord> {
        if event.get("requestBodiesExcluded").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let file_count = event.get("fileCount")?.as_u64()?;
        let bundle_size = event.get("bundleSizeBytes")?.as_u64()?;
        record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationCategory::DiagnosticsMaintenance,
            OperationTrigger::UserAction,
            RecordMetadata::new("导出诊断包", "用户导出了经过脱敏处理的诊断包。", None),
            OperationRecordSpecificDetailsDto::DiagnosticsMaintenance {
                details: DiagnosticsDetailsDto::Export {
                    included_categories: vec![
                        "应用配置摘要".to_owned(),
                        "运行环境摘要".to_owned(),
                        "结构化诊断日志".to_owned(),
                    ],
                    file_count,
                    bundle_size_label: format_bytes(bundle_size),
                    location_label: "用户选择的位置".to_owned(),
                    request_bodies_excluded: true,
                },
            },
            None,
        )
    }

    fn diagnostics_cleanup(&self, event: &Value) -> Option<ProjectedRecord> {
        let deleted_files = event.get("deletedFiles")?.as_u64()?;
        let reclaimed_bytes = event.get("reclaimedBytes")?.as_u64()?;
        record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationCategory::DiagnosticsMaintenance,
            OperationTrigger::UserAction,
            RecordMetadata::new(
                "清理诊断日志",
                "用户清理了本机诊断日志，站点、模型和应用设置未受影响。",
                None,
            ),
            OperationRecordSpecificDetailsDto::DiagnosticsMaintenance {
                details: DiagnosticsDetailsDto::Cleanup {
                    before_size_label: format_bytes(reclaimed_bytes),
                    file_count: deleted_files,
                    after_size_label: format_bytes(0),
                    settings_preserved: true,
                },
            },
            None,
        )
    }

    fn log_limit_change(&self, event: &Value) -> Option<ProjectedRecord> {
        let active = event.get("activeMaxTotalBytes")?.as_u64()?;
        let configured = event.get("configuredMaxTotalBytes")?.as_u64()?;
        let restart_required = event.get("restartRequired")?.as_bool()?;
        let changes = vec![ConfigurationChangeDto {
            field_label: "诊断日志容量".to_owned(),
            before: Some(format_bytes(active)),
            after: Some(if restart_required {
                format!("{}（重启后生效）", format_bytes(configured))
            } else {
                format_bytes(configured)
            }),
        }];
        record(
            exact_time(event.get("timestamp")?.as_str()?)?,
            OperationCategory::ConfigurationChange,
            OperationTrigger::UserAction,
            RecordMetadata::new(
                "调整诊断日志容量",
                "用户修改了诊断日志容量设置。",
                Some("诊断设置".to_owned()),
            ),
            OperationRecordSpecificDetailsDto::ConfigurationChange { changes },
            None,
        )
    }

    fn operation_finished(&mut self, event: Value) -> Option<ProjectedRecord> {
        if event.get("operation").and_then(Value::as_str) == Some("refresh-intelligence-ladder") {
            return intelligence_collection_operation_finished(&event);
        }
        if event.get("operation").and_then(Value::as_str) == Some("refresh-models") {
            let operation_id = event.get("operationId")?.as_str()?;
            if self.non_mutating_operations.remove(operation_id) {
                return None;
            }
        }
        if event.get("fieldScope").and_then(Value::as_array).is_some() {
            return configuration_operation(&event);
        }
        let operation_id = event.get("operationId")?.as_str()?.to_owned();
        if self.pending_legacy_operations.len() < MAX_PENDING_OPERATIONS {
            self.pending_legacy_operations.insert(operation_id, event);
        }
        None
    }

    fn model_sync_confirmation_required(&mut self, event: &Value) -> Option<ProjectedRecord> {
        let operation_id = event.get("operationId")?.as_str()?;
        if self.non_mutating_operations.len() < MAX_PENDING_OPERATIONS {
            self.non_mutating_operations.insert(operation_id.to_owned());
        }
        None
    }

    fn legacy_operation_started(&mut self, event: &Value) -> Option<ProjectedRecord> {
        let operation_id = event.get("operationId")?.as_str()?;
        let mut finished = self.pending_legacy_operations.remove(operation_id)?;
        let object = finished.as_object_mut()?;
        for field in ["targetType", "changeKind", "fieldScope"] {
            object.insert(
                field.to_owned(),
                event.get(field).cloned().unwrap_or(Value::Null),
            );
        }
        configuration_operation(&finished)
    }
}

#[derive(Default)]
struct LifecycleOptions {
    launch_method: Option<String>,
    app_version: Option<String>,
    restored_site_count: Option<u64>,
    restored_model_count: Option<u64>,
    background_tasks_started: Option<bool>,
    settings_saved: Option<bool>,
    window_state_saved: Option<bool>,
    background_tasks_stopped: Option<bool>,
    previous_session_started_at: Option<String>,
}

fn lifecycle_record(
    occurred_at: OperationOccurredAtDto,
    trigger: OperationTrigger,
    action_label: &str,
    description: &str,
    event: LifecycleEvent,
    options: LifecycleOptions,
    exception: Option<OperationExceptionDto>,
) -> Option<ProjectedRecord> {
    record(
        occurred_at,
        OperationCategory::AppLifecycle,
        trigger,
        RecordMetadata::new(action_label, description, Some("RelayPulse".to_owned())),
        OperationRecordSpecificDetailsDto::AppLifecycle {
            event,
            launch_method: options.launch_method,
            app_version: options.app_version,
            restored_site_count: options.restored_site_count,
            restored_model_count: options.restored_model_count,
            background_tasks_started: options.background_tasks_started,
            settings_saved: options.settings_saved,
            window_state_saved: options.window_state_saved,
            background_tasks_stopped: options.background_tasks_stopped,
            previous_session_started_at: options.previous_session_started_at,
            last_completed_action: None,
            failure_source: None,
            crash_report_found: None,
        },
        exception,
    )
}

fn configuration_operation(event: &Value) -> Option<ProjectedRecord> {
    let operation = event.get("operation")?.as_str()?;
    let metadata = operation_metadata(operation)?;
    let outcome = event.get("outcome")?.as_str()?;
    if !matches!(outcome, "succeeded" | "failed") {
        return None;
    }
    let failed = outcome == "failed";
    let field_scope = event.get("fieldScope")?.as_array()?;
    let change_kind = event
        .get("changeKind")
        .and_then(Value::as_str)
        .unwrap_or("update");
    let changes = field_scope
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|field| field_change(field, change_kind, failed))
        .take(MAX_CONFIGURATION_CHANGES)
        .collect::<Vec<_>>();
    if changes.is_empty() {
        return None;
    }
    let action_label = if failed {
        format!("{}失败", metadata.action_stem)
    } else {
        metadata.action_stem.to_owned()
    };
    let description = if failed {
        format!("用户尝试{}，但变更未能提交。", metadata.description_action)
    } else {
        format!("用户已{}。", metadata.description_action)
    };
    let exception = failed
        .then(|| event.get("errorCode").and_then(Value::as_str))
        .flatten()
        .and_then(|code| safe_exception(operation, code));
    if failed && exception.is_none() {
        return None;
    }
    record(
        exact_time(event.get("timestamp")?.as_str()?)?,
        OperationCategory::ConfigurationChange,
        OperationTrigger::UserAction,
        RecordMetadata::new(
            &action_label,
            &description,
            event
                .get("targetType")
                .and_then(Value::as_str)
                .and_then(subject_label)
                .map(ToOwned::to_owned),
        ),
        OperationRecordSpecificDetailsDto::ConfigurationChange { changes },
        exception,
    )
}

struct OperationMetadata {
    action_stem: &'static str,
    description_action: &'static str,
}

fn operation_metadata(operation: &str) -> Option<OperationMetadata> {
    let values = match operation {
        "create-site" => ("新增站点", "新增站点配置"),
        "update-site" => ("调整站点配置", "修改站点配置"),
        "delete-site" => ("删除站点", "删除站点配置"),
        "reorder-sites" => ("调整站点顺序", "调整站点顺序"),
        "set-site-enabled" => ("切换站点状态", "修改站点启用状态"),
        "create-model" => ("新增模型", "新增模型配置"),
        "delete-model" => ("删除模型", "删除模型配置"),
        "set-model-enabled" => ("切换模型状态", "修改模型启用状态"),
        "update-site-advanced" => ("调整站点高级设置", "修改站点高级设置"),
        "update-site-probe-settings" => ("调整站点探测设置", "修改站点探测设置"),
        "update-model-probe-settings" => ("调整模型探测设置", "修改模型探测设置"),
        "update-model-custom-probe-settings" => ("调整模型自定义探测", "修改模型自定义探测设置"),
        "update-general-settings" => ("调整通用设置", "修改通用设置"),
        "refresh-models" => ("同步模型目录", "同步模型目录"),
        "update-notification-settings" => ("调整通知设置", "修改通知设置"),
        "update-intelligence-auto-fetch-settings" => {
            ("调整智力天梯自动获取设置", "修改智力天梯自动获取设置")
        }
        _ => return None,
    };
    Some(OperationMetadata {
        action_stem: values.0,
        description_action: values.1,
    })
}

fn field_change(field: &str, change_kind: &str, failed: bool) -> Option<ConfigurationChangeDto> {
    let (label, sensitive) = match field {
        "name" => ("名称", true),
        "siteConfiguration" => ("站点配置", true),
        "modelConfiguration" => ("模型配置", true),
        "protocol" => ("接口协议", false),
        "baseUrl" => ("基础地址", true),
        "credential" => ("API Key", true),
        "enabled" => ("启用状态", false),
        "priority" => ("站点顺序", false),
        "modelsEndpoint" => ("模型列表路径", true),
        "probeEndpoint" => ("探测路径", true),
        "probeIntervalMinutes" => ("探测间隔", false),
        "slowFirstTokenSeconds" => ("首字超时阈值", false),
        "slowTotalSeconds" => ("总耗时阈值", false),
        "failureThreshold" => ("失败阈值", false),
        "recoveryThreshold" => ("恢复阈值", false),
        "headers" => ("自定义 Header", true),
        "defaultProbeSettings" => ("站点默认探测设置", true),
        "probeSettings" => ("模型探测设置", true),
        "customProbe" => ("自定义探测设置", true),
        "catalog" => ("模型目录", true),
        "launchAtStartup" => ("开机启动", false),
        "continueInBackground" => ("后台运行", false),
        "trayAction" => ("菜单栏行为", false),
        "theme" => ("外观主题", false),
        "language" => ("界面语言", false),
        "historyRetentionDays" => ("记录保留天数", false),
        "diagnosticCaptureEnabled" => ("诊断采集", false),
        "notificationsEnabled" => ("消息通知", false),
        "rules" => ("通知规则", true),
        "autoFetchEnabled" => ("自动获取", false),
        "autoFetchIntervalMinutes" => ("自动获取间隔", false),
        "fetchOnAppStart" => ("启动时获取", false),
        "fetchOnNetworkRestore" => ("网络恢复时获取", false),
        _ => return None,
    };
    if failed {
        return Some(ConfigurationChangeDto {
            field_label: label.to_owned(),
            before: None,
            after: None,
        });
    }
    let (before, after) = if field == "enabled" {
        match change_kind {
            "toggle-on" => (Some("已关闭"), Some("已开启")),
            "toggle-off" => (Some("已开启"), Some("已关闭")),
            "create" => (None, Some("已创建")),
            _ => (None, Some("值已修改")),
        }
    } else {
        match change_kind {
            "create" => (None, Some("已创建")),
            "delete" => (Some("已存在"), Some("已移除")),
            _ if sensitive => (None, Some("已更新")),
            _ => (None, Some("值已修改")),
        }
    };
    Some(ConfigurationChangeDto {
        field_label: label.to_owned(),
        before: before.map(ToOwned::to_owned),
        after: after.map(ToOwned::to_owned),
    })
}

fn safe_exception(operation: &str, code: &str) -> Option<OperationExceptionDto> {
    let (reason, last_step, recommendation) = match code {
        "invalid_input" => (
            "提交的配置未通过校验。",
            "校验配置",
            "请检查配置后重新提交。",
        ),
        "not_found" => ("要修改的配置已不存在。", "查找配置", "请刷新页面后重试。"),
        "credential_unavailable" => (
            "本机凭据暂时不可用。",
            "读取凭据",
            "请检查本机凭据文件后重试。",
        ),
        "database_unavailable" => (
            "本地配置存储暂时不可用。",
            "保存配置",
            "请重新启动 RelayPulse 后重试。",
        ),
        "notification_settings_conflict" | "notification_catalog_changed" => (
            "通知设置已在其他操作中发生变化。",
            "保存通知设置",
            "请刷新页面后重新提交。",
        ),
        "network_error" | "timeout" | "rate_limited" | "upstream_error" => (
            "同步模型目录时上游服务暂时不可用。",
            "同步模型目录",
            "请稍后重试。",
        ),
        _ => return None,
    };
    if matches!(
        code,
        "network_error" | "timeout" | "rate_limited" | "upstream_error"
    ) && operation != "refresh-models"
    {
        return None;
    }
    Some(OperationExceptionDto {
        reason: reason.to_owned(),
        safe_code: Some(code.to_owned()),
        last_step: Some(last_step.to_owned()),
        recommended_action: Some(recommendation.to_owned()),
    })
}

fn intelligence_collection_trigger(value: &str) -> Option<OperationTrigger> {
    match value {
        "manual" => Some(OperationTrigger::UserAction),
        "scheduled" | "application_start" | "network_restore" => Some(OperationTrigger::Automatic),
        _ => None,
    }
}

fn intelligence_collection_exception(code: &str) -> Option<OperationExceptionDto> {
    let (reason, recommendation) = match code {
        "network" => ("获取智力天梯数据时网络不可用。", "请检查网络连接后重试。"),
        "render_timeout" | "worker_timeout" => (
            "智力天梯数据源处理超时。",
            "请稍后重试；如果持续发生，请导出诊断包。",
        ),
        "render_unavailable"
        | "target_not_found"
        | "worker_crashed"
        | "worker_protocol_invalid"
        | "invalid_snapshot"
        | "worker_kill_failed"
        | "cancelled"
        | "database"
        | "internal" => (
            "智力天梯数据采集未能完成。",
            "请稍后重试；如果持续发生，请导出诊断包。",
        ),
        _ => return None,
    };
    Some(OperationExceptionDto {
        reason: reason.to_owned(),
        safe_code: Some(code.to_owned()),
        last_step: Some("采集智力天梯数据".to_owned()),
        recommended_action: Some(recommendation.to_owned()),
    })
}

fn intelligence_collection_operation_finished(event: &Value) -> Option<ProjectedRecord> {
    if event.get("outcome")?.as_str()? != "failed" {
        return None;
    }
    let error_code = event.get("errorCode")?.as_str()?;
    let exception = match error_code {
        "database_unavailable" => intelligence_collection_exception("database")?,
        _ => return None,
    };
    record(
        exact_time(event.get("timestamp")?.as_str()?)?,
        OperationCategory::DataCollection,
        OperationTrigger::UserAction,
        RecordMetadata::new(
            "采集智力天梯数据失败",
            "RelayPulse 未能启动智力天梯数据采集。",
            Some("智力天梯".to_owned()),
        ),
        OperationRecordSpecificDetailsDto::DataCollection {
            source: DataCollectionSource::IntelligenceLadder,
            entry_count: None,
        },
        Some(exception),
    )
}

fn subject_label(target_type: &str) -> Option<&'static str> {
    match target_type {
        "site" => Some("站点配置"),
        "model" => Some("模型配置"),
        "settings" => Some("RelayPulse 设置"),
        "notification-settings" => Some("通知设置"),
        "intelligence-settings" => Some("智力天梯设置"),
        _ => None,
    }
}

struct RecordMetadata {
    action_label: String,
    description: String,
    subject: Option<String>,
}

impl RecordMetadata {
    fn new(action_label: &str, description: &str, subject: Option<String>) -> Self {
        Self {
            action_label: action_label.to_owned(),
            description: description.to_owned(),
            subject,
        }
    }
}

fn record(
    occurred_at: OperationOccurredAtDto,
    category: OperationCategory,
    trigger: OperationTrigger,
    metadata: RecordMetadata,
    detail: OperationRecordSpecificDetailsDto,
    exception: Option<OperationExceptionDto>,
) -> Option<ProjectedRecord> {
    let occurred_at_unix_nanos = OffsetDateTime::parse(&occurred_at.value, &Rfc3339)
        .ok()?
        .unix_timestamp_nanos();
    Some(ProjectedRecord {
        occurred_at_unix_nanos,
        details: OperationRecordDetailsDto {
            record_ref: String::new(),
            occurred_at,
            category,
            trigger,
            action_label: metadata.action_label,
            description: metadata.description,
            subject: metadata.subject,
            detail,
            exception,
        },
    })
}

fn exact_time(value: &str) -> Option<OperationOccurredAtDto> {
    Some(OperationOccurredAtDto {
        value: canonical_time(value)?,
        precision: OccurredAtPrecision::Exact,
        qualifier: None,
    })
}

fn approximate_time(value: &str, qualifier: OccurredAtQualifier) -> Option<OperationOccurredAtDto> {
    Some(OperationOccurredAtDto {
        value: canonical_time(value)?,
        precision: OccurredAtPrecision::Approximate,
        qualifier: Some(qualifier),
    })
}

fn canonical_time(value: &str) -> Option<String> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

fn safe_runtime_token(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn projects_configuration_fields_without_copying_source_values() {
        let marker = "secret-marker-must-not-leak";
        let event = json!({
            "timestamp": "2026-08-08T08:00:00Z",
            "event": "operation-finished",
            "operationId": "op_fixture",
            "operation": "update-site",
            "outcome": "succeeded",
            "targetType": "site",
            "changeKind": "update",
            "fieldScope": ["name", "baseUrl", "credential"],
            "untrusted": marker,
        });

        let record = Projector::new().project(event).expect("record projects");
        let serialized = serde_json::to_string(&record.details).expect("details serialize");

        assert_eq!(record.details.action_label, "调整站点配置");
        assert!(!serialized.contains(marker));
        assert!(!serialized.contains("baseUrl"));
        assert!(!serialized.contains("credential"));
    }

    #[test]
    fn projects_enabled_toggle_direction() {
        for (change_kind, before, after) in [
            ("toggle-on", "已关闭", "已开启"),
            ("toggle-off", "已开启", "已关闭"),
        ] {
            let event = json!({
                "timestamp": "2026-08-08T08:00:00Z",
                "event": "operation-finished",
                "operationId": "op_fixture",
                "operation": "set-site-enabled",
                "outcome": "succeeded",
                "targetType": "site",
                "changeKind": change_kind,
                "fieldScope": ["enabled"],
            });

            let record = Projector::new().project(event).expect("record projects");
            let OperationRecordSpecificDetailsDto::ConfigurationChange { changes } =
                record.details.detail
            else {
                panic!("expected configuration change details");
            };
            assert_eq!(changes[0].field_label, "启用状态");
            assert_eq!(changes[0].before.as_deref(), Some(before));
            assert_eq!(changes[0].after.as_deref(), Some(after));
        }
    }

    #[test]
    fn projects_intelligence_settings_without_copying_values() {
        let event = json!({
            "timestamp": "2026-08-18T08:00:00Z",
            "event": "operation-finished",
            "operationId": "op_intelligence_settings",
            "operation": "update-intelligence-auto-fetch-settings",
            "outcome": "succeeded",
            "targetType": "intelligence-settings",
            "changeKind": "update",
            "fieldScope": ["autoFetchEnabled", "autoFetchIntervalMinutes"],
            "enabled": true,
            "intervalMinutes": 30,
        });

        let record = Projector::new().project(event).expect("record projects");
        let serialized = serde_json::to_string(&record.details).expect("details serialize");
        assert_eq!(record.details.action_label, "调整智力天梯自动获取设置");
        assert!(serialized.contains("自动获取间隔"));
        assert!(!serialized.contains("intervalMinutes"));
    }

    #[test]
    fn model_sync_confirmation_is_not_reported_as_a_configuration_change() {
        let mut projector = Projector::new();
        assert!(projector
            .project(json!({
                "timestamp": "2026-08-18T08:00:00Z",
                "event": "model-sync-confirmation-required",
                "operationId": "op_model_sync",
                "candidateRemovalCount": 2,
            }))
            .is_none());
        assert!(projector
            .project(json!({
                "timestamp": "2026-08-18T08:00:01Z",
                "event": "operation-finished",
                "operationId": "op_model_sync",
                "operation": "refresh-models",
                "outcome": "succeeded",
                "targetType": "site",
                "changeKind": "sync",
                "fieldScope": ["catalog"],
            }))
            .is_none());
    }

    #[test]
    fn excludes_probe_batches_from_operation_history() {
        let event = json!({
            "timestamp": "2026-08-08T08:00:00Z",
            "event": "probe-batch-finished",
            "trigger": "automatic",
            "siteCount": 1,
            "modelCount": 2,
            "completedCount": 2,
            "failedCount": 0,
            "skippedCount": 0,
        });

        assert!(Projector::new().project(event).is_none());
    }

    #[test]
    fn projects_intelligence_ladder_collection_without_duration_or_internal_ids() {
        let event = json!({
            "timestamp": "2026-08-18T12:54:35Z",
            "event": "intelligence-ladder-refresh-finished",
            "operationId": "op_fixture",
            "refreshId": "iq_fixture",
            "trigger": "manual",
            "outcome": "succeeded",
            "durationMs": 7375,
            "entryCount": 19,
            "revision": 931,
            "circuitOpen": false,
        });

        let record = Projector::new().project(event).expect("record projects");
        let serialized = serde_json::to_value(record.details).expect("details serialize");
        assert_eq!(serialized["actionLabel"], "采集智力天梯数据");
        assert_eq!(serialized["trigger"], "user_action");
        assert_eq!(
            serialized["detail"],
            json!({
                "kind": "data_collection",
                "source": "intelligence_ladder",
                "entryCount": 19,
            })
        );
        assert!(serialized.get("durationMs").is_none());
        assert!(serialized.get("operationId").is_none());
        assert!(serialized.get("refreshId").is_none());
    }

    #[test]
    fn projects_safe_intelligence_collection_failures_and_ignores_non_failures() {
        let failed = json!({
            "timestamp": "2026-08-18T12:54:35Z",
            "event": "intelligence-ladder-refresh-finished",
            "trigger": "scheduled",
            "outcome": "failed",
            "errorCode": "invalid_snapshot",
        });
        let record = Projector::new().project(failed).expect("record projects");
        let serialized = serde_json::to_value(record.details).expect("details serialize");
        assert_eq!(serialized["trigger"], "automatic");
        assert_eq!(serialized["exception"]["safeCode"], "invalid_snapshot");
        assert_eq!(
            serialized["detail"],
            json!({
                "kind": "data_collection",
                "source": "intelligence_ladder",
            })
        );

        for reason in ["already_running", "trigger_disabled"] {
            assert!(Projector::new()
                .project(json!({
                    "timestamp": "2026-08-18T12:55:00Z",
                    "event": "intelligence-ladder-refresh-skipped",
                    "trigger": "scheduled",
                    "reason": reason,
                }))
                .is_none());
        }
    }

    #[test]
    fn projects_circuit_open_as_an_intelligence_collection_failure() {
        let event = json!({
            "timestamp": "2026-08-18T12:55:00Z",
            "event": "intelligence-ladder-refresh-skipped",
            "trigger": "application_start",
            "reason": "circuit_open",
        });

        let record = Projector::new().project(event).expect("record projects");
        let serialized = serde_json::to_value(record.details).expect("details serialize");
        assert_eq!(serialized["trigger"], "automatic");
        assert_eq!(serialized["exception"]["safeCode"], "circuit_open");
    }

    #[test]
    fn projects_command_failure_before_intelligence_collection_starts() {
        let event = json!({
            "timestamp": "2026-08-18T12:55:00Z",
            "event": "operation-finished",
            "operationId": "op_fixture",
            "operation": "refresh-intelligence-ladder",
            "outcome": "failed",
            "errorCode": "database_unavailable",
        });

        let record = Projector::new().project(event).expect("record projects");
        let serialized = serde_json::to_value(record.details).expect("details serialize");
        assert_eq!(serialized["exception"]["safeCode"], "database");
        assert_eq!(serialized["actionLabel"], "采集智力天梯数据失败");
    }

    #[test]
    fn projects_all_non_configuration_detail_variants_from_fixed_evidence() {
        let cases = [
            (
                json!({
                    "timestamp": "2026-08-08T08:00:00Z",
                    "event": "application-startup-completed",
                    "applicationVersion": "0.1.0",
                    "restoredSiteCount": 2,
                    "restoredModelCount": 3,
                    "backgroundTasksStarted": true,
                }),
                "app_lifecycle",
                "started",
            ),
            (
                json!({
                    "timestamp": "2026-08-08T08:01:00Z",
                    "event": "intelligence-ladder-refresh-finished",
                    "trigger": "scheduled",
                    "outcome": "succeeded",
                    "entryCount": 19,
                }),
                "data_collection",
                "automatic",
            ),
            (
                json!({
                    "timestamp": "2026-08-08T08:02:00Z",
                    "event": "diagnostics-exported",
                    "fileCount": 4,
                    "bundleSizeBytes": 4096,
                    "locationKind": "user_selected",
                    "requestBodiesExcluded": true,
                }),
                "diagnostics_maintenance",
                "export",
            ),
        ];

        for (event, kind, evidence) in cases {
            let record = Projector::new().project(event).expect("record projects");
            let serialized = serde_json::to_value(record.details).expect("details serialize");
            assert_eq!(serialized["detail"]["kind"], kind);
            assert!(
                serialized["detail"]
                    == json!({
                        "kind": "app_lifecycle",
                        "event": evidence,
                        "appVersion": "0.1.0",
                        "restoredSiteCount": 2,
                        "restoredModelCount": 3,
                        "backgroundTasksStarted": true,
                        "launchMethod": "用户打开",
                    })
                    || serialized["detail"]["action"] == evidence
                    || serialized["trigger"] == evidence
            );
        }
    }

    #[test]
    fn excludes_individual_probes_and_unsafe_export_evidence() {
        for event in [
            json!({
                "timestamp": "2026-08-08T08:00:00Z",
                "event": "probe-started",
                "probeId": "probe_fixture",
                "runKind": "manual",
                "protocol": "openai-compatible",
            }),
            json!({
                "timestamp": "2026-08-08T08:00:01Z",
                "event": "probe-finished",
                "probeId": "probe_fixture",
                "status": "healthy",
                "durationMs": 1,
            }),
            json!({
                "timestamp": "2026-08-08T08:02:00Z",
                "event": "diagnostics-exported",
                "fileCount": 4,
                "bundleSizeBytes": 4096,
                "locationKind": "user_selected",
                "requestBodiesExcluded": false,
            }),
        ] {
            assert!(Projector::new().project(event).is_none());
        }
    }
}
