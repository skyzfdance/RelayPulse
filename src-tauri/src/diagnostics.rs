use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    application::ApplicationSnapshot,
    domain::{ProbeRunKind, ProbeStatus, ResponseMode, SiteProtocol},
    startup::{StartupErrorCode, StartupFailure},
};

const LOG_DIRECTORY_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "relaypulse.log";
const RUN_MARKER_FILE_NAME: &str = "relaypulse.run.json";
const DIAGNOSTIC_SETTINGS_FILE_NAME: &str = "relaypulse.diagnostics.json";
const BYTES_PER_MEGABYTE: u64 = 1024 * 1024;
const MAX_LOG_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const MIN_LOG_TOTAL_MEGABYTES: u16 = 10;
pub(crate) const MAX_LOG_TOTAL_MEGABYTES: u16 = 100;
const DEFAULT_LOG_TOTAL_MEGABYTES: u16 = 10;
const MAX_EXPORTED_LOG_EVENTS: usize = 2_000;
const MAX_EXPORTED_FAILURES: usize = 100;
const MAX_FRONTEND_STACK_FRAMES: usize = 12;

static ACTIVE_DIAGNOSTICS: OnceLock<Mutex<Option<Diagnostics>>> = OnceLock::new();
static PANIC_HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub(crate) struct OperationAudit {
    pub target_type: &'static str,
    pub target_id: Option<String>,
    pub parent_id: Option<String>,
    pub change_kind: &'static str,
    pub field_scope: &'static [&'static str],
}

pub(crate) struct FrontendErrorRecord<'a> {
    pub error_id: &'a str,
    pub source: &'a str,
    pub view: &'a str,
    pub error_name: &'a str,
    pub fingerprint: &'a str,
    pub stack_frames: &'a [String],
    pub last_action_id: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunMarker {
    session_id: String,
    started_at: String,
    application_version: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticSettings {
    max_total_megabytes: u16,
}

#[derive(Debug)]
struct PreviousRun {
    session_id: Option<String>,
    started_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatistics {
    pub total_bytes: u64,
    pub file_count: u64,
    pub max_total_bytes: u64,
    pub configured_max_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogCleanupResult {
    pub deleted_files: u64,
    pub reclaimed_bytes: u64,
    pub statistics: LogStatistics,
}

#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<DiagnosticsInner>,
}

struct DiagnosticsInner {
    log_directory: PathBuf,
    log_file: PathBuf,
    run_marker_file: PathBuf,
    settings_file: PathBuf,
    session_id: String,
    max_total_bytes: u64,
    max_rotated_log_files: usize,
    configured_max_total_bytes: AtomicU64,
    write_lock: Mutex<()>,
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !PANIC_HOOK_ACTIVE.swap(true, Ordering::AcqRel) {
            if let Ok(active) = active_diagnostics().try_lock() {
                if let Some(diagnostics) = active.as_ref() {
                    let location = info.location();
                    diagnostics.record_native_panic(
                        std::thread::current().name(),
                        location.map(|value| value.file()),
                        location.map(|value| value.line()),
                        location.map(|value| value.column()),
                    );
                }
            }
            PANIC_HOOK_ACTIVE.store(false, Ordering::Release);
        }
        previous(info);
    }));
}

fn active_diagnostics() -> &'static Mutex<Option<Diagnostics>> {
    ACTIVE_DIAGNOSTICS.get_or_init(|| Mutex::new(None))
}

impl Diagnostics {
    pub fn initialize(data_directory: &Path) -> io::Result<Self> {
        let log_directory = data_directory.join(LOG_DIRECTORY_NAME);
        fs::create_dir_all(&log_directory)?;
        let run_marker_file = data_directory.join(RUN_MARKER_FILE_NAME);
        let settings_file = data_directory.join(DIAGNOSTIC_SETTINGS_FILE_NAME);
        let (max_total_megabytes, settings_read_failed) =
            match read_diagnostic_settings(&settings_file) {
                Ok(Some(settings)) => (settings.max_total_megabytes, false),
                Ok(None) => (DEFAULT_LOG_TOTAL_MEGABYTES, false),
                Err(_) => (DEFAULT_LOG_TOTAL_MEGABYTES, true),
            };
        let max_total_bytes = megabytes_to_bytes(max_total_megabytes);
        let previous_run = read_previous_run(&run_marker_file);
        let session_id = format!("session_{}", Uuid::new_v4().simple());
        let diagnostics = Self {
            inner: Arc::new(DiagnosticsInner {
                log_file: log_directory.join(LOG_FILE_NAME),
                log_directory,
                run_marker_file,
                settings_file,
                session_id,
                max_total_bytes,
                max_rotated_log_files: max_rotated_log_files(max_total_bytes),
                configured_max_total_bytes: AtomicU64::new(max_total_bytes),
                write_lock: Mutex::new(()),
            }),
        };
        diagnostics.remove_excess_rotated_logs()?;
        diagnostics.enforce_total_capacity(0)?;
        let marker_written = diagnostics.write_run_marker().is_ok();
        if let Ok(mut active) = active_diagnostics().lock() {
            *active = Some(diagnostics.clone());
        }
        diagnostics.record_startup();
        if let Some(previous_run) = previous_run {
            diagnostics.record_previous_session_terminated_unexpectedly(&previous_run);
        }
        if !marker_written {
            diagnostics.record_background_error("run-marker", "run_marker_write_failed");
        }
        if settings_read_failed {
            diagnostics
                .record_background_error("diagnostic-settings", "diagnostic_settings_invalid");
        }
        Ok(diagnostics)
    }

    pub fn log_directory(&self) -> &Path {
        &self.inner.log_directory
    }

    pub fn record_startup(&self) {
        self.append(json!({
            "level": "info",
            "event": "application-started",
            "applicationVersion": env!("CARGO_PKG_VERSION"),
            "operatingSystem": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "activeMaxTotalBytes": self.inner.max_total_bytes,
        }));
    }

    pub fn mark_clean_exit(&self) {
        self.append(json!({
            "level": "info",
            "event": "application-exited",
            "outcome": "clean",
        }));
        if remove_if_exists(&self.inner.run_marker_file).is_err() {
            self.record_background_error("run-marker", "run_marker_cleanup_failed");
        }
    }

    fn record_previous_session_terminated_unexpectedly(&self, previous: &PreviousRun) {
        self.append(json!({
            "level": "error",
            "event": "previous-session-terminated-unexpectedly",
            "previousSessionId": previous.session_id,
            "previousStartedAt": previous.started_at,
        }));
    }

    fn record_native_panic(
        &self,
        thread_name: Option<&str>,
        source_file: Option<&str>,
        line: Option<u32>,
        column: Option<u32>,
    ) {
        self.try_append(json!({
            "level": "error",
            "event": "native-panic",
            "thread": thread_name.and_then(sanitize_runtime_token),
            "sourceFile": source_file.and_then(sanitize_source_file),
            "line": line,
            "column": column,
        }));
    }

    pub fn record_startup_failed(&self, failure: &StartupFailure) {
        self.append(json!({
            "level": "error",
            "event": "application-startup-failed",
            "errorCode": startup_error_name(failure.code()),
        }));
    }

    pub fn record_state_change(&self, operation_id: Option<&str>, revision: u64, domains: &[&str]) {
        self.append(json!({
            "event": "state-changed",
            "operationId": operation_id,
            "revision": revision,
            "domains": domains,
        }));
    }

    pub fn record_records_cleared(&self, operation_id: Option<&str>, deleted: u64, revision: u64) {
        self.append(json!({
            "event": "probe-records-cleared",
            "operationId": operation_id,
            "deleted": deleted,
            "revision": revision,
        }));
    }

    pub fn record_logs_cleared(
        &self,
        operation_id: Option<&str>,
        deleted_files: u64,
        reclaimed_bytes: u64,
    ) {
        self.append(json!({
            "level": "info",
            "event": "diagnostic-logs-cleared",
            "operationId": operation_id,
            "deletedFiles": deleted_files,
            "reclaimedBytes": reclaimed_bytes,
        }));
    }

    pub fn record_log_directory_opened(&self, operation_id: Option<&str>) {
        self.append(json!({
            "event": "log-directory-opened",
            "operationId": operation_id,
        }));
    }

    pub fn record_log_limit_configured(&self, operation_id: &str, configured_max_total_bytes: u64) {
        self.append(json!({
            "level": "info",
            "event": "diagnostic-log-limit-configured",
            "operationId": operation_id,
            "activeMaxTotalBytes": self.inner.max_total_bytes,
            "configuredMaxTotalBytes": configured_max_total_bytes,
            "restartRequired": configured_max_total_bytes != self.inner.max_total_bytes,
        }));
    }

    pub fn record_diagnostics_exported(&self, operation_id: Option<&str>) {
        self.append(json!({
            "event": "diagnostics-exported",
            "operationId": operation_id,
        }));
    }

    pub fn record_window_opened(&self, operation_id: Option<&str>, window: &'static str) {
        self.append(json!({
            "event": "window-opened",
            "operationId": operation_id,
            "window": window,
        }));
    }

    pub fn record_autostart_reconciled(&self, enabled: bool, succeeded: bool) {
        self.append(json!({
            "event": "autostart-reconciled",
            "enabled": enabled,
            "succeeded": succeeded,
        }));
    }

    pub(crate) fn record_operation_started(
        &self,
        operation_id: &str,
        operation: &'static str,
        audit: Option<&OperationAudit>,
    ) {
        let target_id = audit
            .and_then(|value| value.target_id.as_deref())
            .and_then(sanitize_audit_identifier);
        let parent_id = audit
            .and_then(|value| value.parent_id.as_deref())
            .and_then(sanitize_audit_identifier);
        self.append(json!({
            "level": "info",
            "event": "operation-started",
            "operationId": operation_id,
            "operation": operation,
            "targetType": audit.map(|value| value.target_type),
            "targetId": target_id,
            "parentId": parent_id,
            "changeKind": audit.map(|value| value.change_kind),
            "fieldScope": audit.map(|value| value.field_scope),
        }));
    }

    pub fn record_frontend_action(
        &self,
        action_id: &str,
        action: &str,
        view: &str,
        destination_view: Option<&str>,
        target_type: Option<&str>,
        target_id: Option<&str>,
    ) {
        self.append(json!({
            "level": "info",
            "event": "frontend-action",
            "actionId": action_id,
            "action": action,
            "view": view,
            "destinationView": destination_view,
            "targetType": target_type,
            "targetId": target_id,
        }));
    }

    pub(crate) fn record_frontend_error(&self, record: FrontendErrorRecord<'_>) {
        self.append(json!({
            "level": "error",
            "event": "frontend-error",
            "errorId": record.error_id,
            "source": record.source,
            "view": record.view,
            "errorName": record.error_name,
            "fingerprint": record.fingerprint,
            "stackFrames": record.stack_frames.iter().take(MAX_FRONTEND_STACK_FRAMES).collect::<Vec<_>>(),
            "lastActionId": record.last_action_id,
        }));
    }

    pub fn record_operation_finished(
        &self,
        operation_id: &str,
        operation: &'static str,
        duration_ms: u64,
        error_code: Option<&str>,
    ) {
        let outcome = if error_code.is_some() {
            "failed"
        } else {
            "succeeded"
        };
        self.append(json!({
            "level": if error_code.is_some() { "error" } else { "info" },
            "event": "operation-finished",
            "operationId": operation_id,
            "operation": operation,
            "outcome": outcome,
            "durationMs": duration_ms,
            "errorCode": error_code,
        }));
    }

    pub fn record_probe_started(
        &self,
        probe_id: &str,
        operation_id: Option<&str>,
        run_kind: ProbeRunKind,
        protocol: SiteProtocol,
    ) {
        self.append(json!({
            "level": "info",
            "event": "probe-started",
            "probeId": probe_id,
            "operationId": operation_id,
            "runKind": run_kind_name(run_kind),
            "protocol": protocol_name(protocol),
        }));
    }

    pub fn record_probe_finished(
        &self,
        probe_id: &str,
        operation_id: Option<&str>,
        status: ProbeStatus,
        http_status: Option<u16>,
        duration_ms: u32,
        error_code: Option<&str>,
    ) {
        self.append(json!({
            "level": if matches!(status, ProbeStatus::Healthy | ProbeStatus::Slow) {
                "info"
            } else {
                "error"
            },
            "event": "probe-finished",
            "probeId": probe_id,
            "operationId": operation_id,
            "status": status_name(status),
            "httpStatus": http_status,
            "durationMs": duration_ms,
            "errorCode": error_code,
        }));
    }

    pub fn record_background_error(&self, operation: &'static str, error_code: &'static str) {
        self.append(json!({
            "level": "error",
            "event": "background-operation-failed",
            "operation": operation,
            "errorCode": error_code,
        }));
    }

    pub fn statistics(&self) -> io::Result<LogStatistics> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("diagnostics lock poisoned"))?;
        self.statistics_unlocked()
    }

    pub fn configure_max_total_megabytes(
        &self,
        max_total_megabytes: u16,
    ) -> io::Result<LogStatistics> {
        if !(MIN_LOG_TOTAL_MEGABYTES..=MAX_LOG_TOTAL_MEGABYTES).contains(&max_total_megabytes) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "diagnostic log limit is out of range",
            ));
        }
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("diagnostics lock poisoned"))?;
        write_diagnostic_settings(
            &self.inner.settings_file,
            DiagnosticSettings {
                max_total_megabytes,
            },
        )?;
        self.inner
            .configured_max_total_bytes
            .store(megabytes_to_bytes(max_total_megabytes), Ordering::Release);
        self.statistics_unlocked()
    }

    pub fn clear_logs(&self) -> io::Result<LogCleanupResult> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("diagnostics lock poisoned"))?;
        let before = self.statistics_unlocked()?;
        let mut deleted_files = 0u64;
        for path in self.log_paths_oldest_first() {
            match fs::remove_file(path) {
                Ok(()) => deleted_files = deleted_files.saturating_add(1),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(LogCleanupResult {
            deleted_files,
            reclaimed_bytes: before.total_bytes,
            statistics: self.statistics_unlocked()?,
        })
    }

    pub fn export(
        &self,
        path: &Path,
        snapshot: Option<&ApplicationSnapshot>,
        startup_failure: Option<&StartupFailure>,
    ) -> io::Result<()> {
        let mut protocols = BTreeMap::<&'static str, usize>::new();
        let mut statuses = BTreeMap::<&'static str, usize>::new();
        let mut model_count = 0usize;
        let mut enabled_model_count = 0usize;
        let mut enabled_site_count = 0usize;
        let mut credential_site_count = 0usize;

        if let Some(snapshot) = snapshot {
            for site in &snapshot.sites {
                *protocols
                    .entry(protocol_name(site.site.protocol))
                    .or_default() += 1;
                model_count += site.models.len();
                enabled_model_count += site
                    .models
                    .iter()
                    .filter(|model| model.model.enabled)
                    .count();
                enabled_site_count += usize::from(site.site.enabled);
                credential_site_count += usize::from(site.site.credential_ref.is_some());
            }
            for record in &snapshot.probe_records {
                *statuses
                    .entry(status_name(record.result.status))
                    .or_default() += 1;
            }
        }

        let log_statistics = self.statistics()?;
        let snapshot_summary = snapshot.map(|snapshot| {
            json!({
                "available": true,
                "revision": snapshot.revision,
                "siteCount": snapshot.sites.len(),
                "enabledSiteCount": enabled_site_count,
                "modelCount": model_count,
                "enabledModelCount": enabled_model_count,
                "sitesWithCredentialCount": credential_site_count,
                "probeRecordCount": snapshot.probe_records.len(),
                "protocolCounts": protocols,
                "probeStatusCounts": statuses,
                "trayEnabled": snapshot.general_settings.tray_action
                    == crate::domain::TrayAction::Panel,
                "historyRetentionDays": snapshot.general_settings.history_retention_days,
                "diagnosticCaptureEnabled": snapshot.general_settings.diagnostic_capture_enabled,
            })
        }).unwrap_or_else(|| json!({ "available": false }));
        let recent_failures = snapshot.map(recent_probe_failures).unwrap_or_default();

        let package = json!({
            "schemaVersion": 2,
            "generatedAt": timestamp(),
            "application": {
                "name": "RelayPulse",
                "version": env!("CARGO_PKG_VERSION"),
                "operatingSystem": std::env::consts::OS,
                "architecture": std::env::consts::ARCH,
            },
            "backend": {
                "ready": snapshot.is_some(),
                "startupErrorCode": startup_failure.map(|failure| startup_error_name(failure.code())),
            },
            "storage": log_statistics,
            "snapshot": snapshot_summary,
            "recentProbeFailures": recent_failures,
            "logEvents": self.log_events(),
            "exclusions": [
                "API credentials and credential references",
                "SQLite database contents",
                "site names, model names, URLs, configured headers, and prompts",
                "request and response bodies, including streaming chunk contents",
                "frontend error messages, form values, and search terms",
            ],
        });

        let file = private_create_file(path)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        serde_json::to_writer_pretty(&mut encoder, &package).map_err(io::Error::other)?;
        encoder.finish()?;
        Ok(())
    }

    fn log_events(&self) -> Vec<Value> {
        let mut events = Vec::new();
        for path in self.log_paths_oldest_first() {
            let Ok(file) = File::open(path) else { continue };
            events.extend(
                BufReader::new(file)
                    .lines()
                    .map_while(Result::ok)
                    .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
                    .filter_map(sanitize_log_event),
            );
        }
        events
            .into_iter()
            .rev()
            .take(MAX_EXPORTED_LOG_EVENTS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn append(&self, mut entry: Value) {
        let Ok(_guard) = self.inner.write_lock.lock() else {
            return;
        };
        self.append_unlocked(&mut entry);
    }

    fn try_append(&self, mut entry: Value) {
        let Ok(_guard) = self.inner.write_lock.try_lock() else {
            return;
        };
        self.append_unlocked(&mut entry);
    }

    fn append_unlocked(&self, entry: &mut Value) {
        if let Some(object) = entry.as_object_mut() {
            object.insert("schemaVersion".to_owned(), Value::from(1));
            object.insert("timestamp".to_owned(), Value::String(timestamp()));
            object.insert(
                "sessionId".to_owned(),
                Value::String(self.inner.session_id.clone()),
            );
        }
        let Ok(mut line) = serde_json::to_vec(&entry) else {
            return;
        };
        line.push(b'\n');
        let Ok(incoming_bytes) = u64::try_from(line.len()) else {
            return;
        };
        if incoming_bytes > MAX_LOG_FILE_BYTES || self.rotate_if_needed(incoming_bytes).is_err() {
            return;
        }
        let Ok(mut file) = private_append_file(&self.inner.log_file) else {
            return;
        };
        let _ = file.write_all(&line);
    }

    fn write_run_marker(&self) -> io::Result<()> {
        let marker = RunMarker {
            session_id: self.inner.session_id.clone(),
            started_at: timestamp(),
            application_version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        let mut file = private_create_file(&self.inner.run_marker_file)?;
        serde_json::to_writer(&mut file, &marker).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()
    }

    fn rotate_if_needed(&self, incoming_bytes: u64) -> io::Result<()> {
        if let Ok(metadata) = fs::metadata(&self.inner.log_file) {
            if metadata.len().saturating_add(incoming_bytes) > MAX_LOG_FILE_BYTES {
                let oldest = self.rotated_log_path(self.inner.max_rotated_log_files);
                remove_if_exists(&oldest)?;
                for index in (2..=self.inner.max_rotated_log_files).rev() {
                    let source = self.rotated_log_path(index - 1);
                    let target = self.rotated_log_path(index);
                    rename_if_exists(&source, &target)?;
                }
                fs::rename(&self.inner.log_file, self.rotated_log_path(1))?;
            }
        }
        self.enforce_total_capacity(incoming_bytes)
    }

    fn rotated_log_path(&self, index: usize) -> PathBuf {
        self.inner
            .log_directory
            .join(format!("{LOG_FILE_NAME}.{index}"))
    }

    fn log_paths_oldest_first(&self) -> Vec<PathBuf> {
        let mut paths = (1..=self.inner.max_rotated_log_files)
            .rev()
            .map(|index| self.rotated_log_path(index))
            .collect::<Vec<_>>();
        paths.push(self.inner.log_file.clone());
        paths
    }

    fn statistics_unlocked(&self) -> io::Result<LogStatistics> {
        let mut total_bytes = 0u64;
        let mut file_count = 0u64;
        for path in self.log_paths_oldest_first() {
            match fs::metadata(path) {
                Ok(metadata) if metadata.is_file() => {
                    total_bytes = total_bytes.saturating_add(metadata.len());
                    file_count = file_count.saturating_add(1);
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(LogStatistics {
            total_bytes,
            file_count,
            max_total_bytes: self.inner.max_total_bytes,
            configured_max_total_bytes: self
                .inner
                .configured_max_total_bytes
                .load(Ordering::Acquire),
        })
    }

    fn remove_excess_rotated_logs(&self) -> io::Result<()> {
        for entry in fs::read_dir(&self.inner.log_directory)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            let Some(index) = name
                .strip_prefix(&format!("{LOG_FILE_NAME}."))
                .and_then(|value| value.parse::<usize>().ok())
            else {
                continue;
            };
            if index > self.inner.max_rotated_log_files {
                remove_if_exists(&entry.path())?;
            }
        }
        Ok(())
    }

    fn enforce_total_capacity(&self, incoming_bytes: u64) -> io::Result<()> {
        let mut total_bytes = self.statistics_unlocked()?.total_bytes;
        while total_bytes.saturating_add(incoming_bytes) > self.inner.max_total_bytes {
            let Some(path) = (1..=self.inner.max_rotated_log_files)
                .rev()
                .map(|index| self.rotated_log_path(index))
                .find(|path| path.is_file())
            else {
                break;
            };
            let bytes = fs::metadata(&path)?.len();
            remove_if_exists(&path)?;
            total_bytes = total_bytes.saturating_sub(bytes);
        }
        Ok(())
    }
}

fn megabytes_to_bytes(megabytes: u16) -> u64 {
    u64::from(megabytes).saturating_mul(BYTES_PER_MEGABYTE)
}

fn max_rotated_log_files(max_total_bytes: u64) -> usize {
    let managed_files = max_total_bytes.saturating_add(MAX_LOG_FILE_BYTES - 1) / MAX_LOG_FILE_BYTES;
    usize::try_from(managed_files.saturating_sub(1)).unwrap_or(usize::MAX)
}

fn read_diagnostic_settings(path: &Path) -> io::Result<Option<DiagnosticSettings>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let settings = serde_json::from_str::<DiagnosticSettings>(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if !(MIN_LOG_TOTAL_MEGABYTES..=MAX_LOG_TOTAL_MEGABYTES).contains(&settings.max_total_megabytes)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "diagnostic log limit is out of range",
        ));
    }
    Ok(Some(settings))
}

fn write_diagnostic_settings(path: &Path, settings: DiagnosticSettings) -> io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    let result = (|| {
        let mut file = private_create_file(&temporary)?;
        serde_json::to_writer(&mut file, &settings).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.flush()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = remove_if_exists(&temporary);
    }
    result
}

fn sanitize_log_event(value: Value) -> Option<Value> {
    let event = value.get("event")?.as_str()?;
    let timestamp = value.get("timestamp")?.as_str()?;
    let session_id = value.get("sessionId").and_then(Value::as_str);
    match event {
        "application-started" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "applicationVersion": value.get("applicationVersion")?.as_str()?,
            "operatingSystem": value.get("operatingSystem")?.as_str()?,
            "architecture": value.get("architecture")?.as_str()?,
            "activeMaxTotalBytes": value.get("activeMaxTotalBytes").and_then(Value::as_u64),
        })),
        "application-startup-failed" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "errorCode": value.get("errorCode")?.as_str()?,
        })),
        "application-exited" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "outcome": "clean",
        })),
        "previous-session-terminated-unexpectedly" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "previousSessionId": value.get("previousSessionId").and_then(Value::as_str),
            "previousStartedAt": value.get("previousStartedAt").and_then(Value::as_str),
        })),
        "native-panic" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "thread": value.get("thread").and_then(Value::as_str),
            "sourceFile": value.get("sourceFile").and_then(Value::as_str),
            "line": value.get("line").and_then(Value::as_u64),
            "column": value.get("column").and_then(Value::as_u64),
        })),
        "log-directory-opened" | "diagnostics-exported" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operationId": value.get("operationId").and_then(Value::as_str),
        })),
        "state-changed" => {
            let revision = value.get("revision")?.as_u64()?;
            let domains = value
                .get("domains")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .filter(|domain| {
                    matches!(
                        *domain,
                        "sites" | "models" | "records" | "status" | "settings"
                    )
                })
                .collect::<Vec<_>>();
            Some(json!({
                "timestamp": timestamp,
                "sessionId": session_id,
                "event": event,
                "operationId": value.get("operationId").and_then(Value::as_str),
                "revision": revision,
                "domains": domains,
            }))
        }
        "probe-records-cleared" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operationId": value.get("operationId").and_then(Value::as_str),
            "deleted": value.get("deleted")?.as_u64()?,
            "revision": value.get("revision")?.as_u64()?,
        })),
        "diagnostic-logs-cleared" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operationId": value.get("operationId").and_then(Value::as_str),
            "deletedFiles": value.get("deletedFiles")?.as_u64()?,
            "reclaimedBytes": value.get("reclaimedBytes")?.as_u64()?,
        })),
        "diagnostic-log-limit-configured" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operationId": value.get("operationId")?.as_str()?,
            "activeMaxTotalBytes": value.get("activeMaxTotalBytes")?.as_u64()?,
            "configuredMaxTotalBytes": value.get("configuredMaxTotalBytes")?.as_u64()?,
            "restartRequired": value.get("restartRequired")?.as_bool()?,
        })),
        "autostart-reconciled" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "enabled": value.get("enabled")?.as_bool()?,
            "succeeded": value.get("succeeded")?.as_bool()?,
        })),
        "window-opened" => {
            let window = value.get("window")?.as_str()?;
            if !matches!(window, "main" | "status-panel") {
                return None;
            }
            Some(json!({
                "timestamp": timestamp,
                "sessionId": session_id,
                "event": event,
                "operationId": value.get("operationId").and_then(Value::as_str),
                "window": window,
            }))
        }
        "operation-started" => {
            let field_scope = value
                .get("fieldScope")
                .and_then(Value::as_array)
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|field| is_safe_audit_field(field))
                        .collect::<Vec<_>>()
                });
            Some(json!({
                "timestamp": timestamp,
                "sessionId": session_id,
                "event": event,
                "operationId": value.get("operationId")?.as_str()?,
                "operation": value.get("operation")?.as_str()?,
                "targetType": value.get("targetType").and_then(Value::as_str).filter(|value| is_safe_target_type(value)),
                "targetId": value.get("targetId").and_then(Value::as_str).and_then(sanitize_audit_identifier),
                "parentId": value.get("parentId").and_then(Value::as_str).and_then(sanitize_audit_identifier),
                "changeKind": value.get("changeKind").and_then(Value::as_str).filter(|value| is_safe_change_kind(value)),
                "fieldScope": field_scope,
            }))
        }
        "operation-finished" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operationId": value.get("operationId")?.as_str()?,
            "operation": value.get("operation")?.as_str()?,
            "outcome": value.get("outcome")?.as_str()?,
            "durationMs": value.get("durationMs")?.as_u64()?,
            "errorCode": value.get("errorCode").and_then(Value::as_str),
        })),
        "probe-started" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "probeId": value.get("probeId")?.as_str()?,
            "operationId": value.get("operationId").and_then(Value::as_str),
            "runKind": value.get("runKind")?.as_str()?,
            "protocol": value.get("protocol")?.as_str()?,
        })),
        "probe-finished" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "probeId": value.get("probeId")?.as_str()?,
            "operationId": value.get("operationId").and_then(Value::as_str),
            "status": value.get("status")?.as_str()?,
            "httpStatus": value.get("httpStatus").and_then(Value::as_u64),
            "durationMs": value.get("durationMs")?.as_u64()?,
            "errorCode": value.get("errorCode").and_then(Value::as_str),
        })),
        "background-operation-failed" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "operation": value.get("operation")?.as_str()?,
            "errorCode": value.get("errorCode")?.as_str()?,
        })),
        "frontend-action" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "actionId": value.get("actionId")?.as_str()?,
            "action": value.get("action")?.as_str()?.to_owned(),
            "view": value.get("view")?.as_str()?,
            "destinationView": value.get("destinationView").and_then(Value::as_str),
            "targetType": value.get("targetType").and_then(Value::as_str),
            "targetId": value.get("targetId").and_then(Value::as_str).and_then(sanitize_audit_identifier),
        })),
        "frontend-error" => Some(json!({
            "timestamp": timestamp,
            "sessionId": session_id,
            "event": event,
            "errorId": value.get("errorId")?.as_str()?,
            "source": value.get("source")?.as_str()?,
            "view": value.get("view")?.as_str()?,
            "errorName": value.get("errorName")?.as_str()?,
            "fingerprint": value.get("fingerprint")?.as_str()?,
            "stackFrames": value.get("stackFrames")?.as_array()?.iter().filter_map(Value::as_str).take(MAX_FRONTEND_STACK_FRAMES).collect::<Vec<_>>(),
            "lastActionId": value.get("lastActionId").and_then(Value::as_str),
        })),
        _ => None,
    }
}

fn recent_probe_failures(snapshot: &ApplicationSnapshot) -> Vec<Value> {
    snapshot
        .probe_records
        .iter()
        .filter(|record| {
            !matches!(
                record.result.status,
                ProbeStatus::Healthy | ProbeStatus::Slow
            )
        })
        .take(MAX_EXPORTED_FAILURES)
        .map(|record| {
            json!({
                "probeId": record.result.id.as_str(),
                "runKind": run_kind_name(record.result.run_kind),
                "startedAtMs": record.result.started_at.as_i64(),
                "finishedAtMs": record.result.finished_at.map(|value| value.as_i64()),
                "status": status_name(record.result.status),
                "httpStatus": record.result.http_status,
                "responseHeadersMs": record.result.response_headers_ms,
                "firstTokenMs": record.result.first_token_ms,
                "totalTimeMs": record.result.total_time_ms,
                "errorCode": record.result.error_code,
                "endedNormally": record.result.ended_normally,
                "retryCount": record.result.retry_count,
                "providerRequestId": record.result.request_id,
                "capture": {
                    "requestCapturedBytes": record.capture.request_captured_bytes,
                    "requestLimitBytes": record.capture.request_limit_bytes,
                    "requestTruncated": record.capture.request_truncated,
                    "responseMode": record.capture.response_mode.map(response_mode_name),
                    "responseContentType": record.capture.response_content_type,
                    "responseCapturedBytes": record.capture.response_captured_bytes,
                    "responseLimitBytes": record.capture.response_limit_bytes,
                    "responseTruncated": record.capture.response_truncated,
                },
            })
        })
        .collect()
}

fn is_safe_target_type(value: &str) -> bool {
    matches!(value, "site" | "model" | "settings" | "probe-records")
}

fn sanitize_audit_identifier(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    .then(|| value.to_owned())
}

fn is_safe_change_kind(value: &str) -> bool {
    matches!(
        value,
        "create" | "update" | "delete" | "reorder" | "toggle" | "sync" | "clear"
    )
}

fn is_safe_audit_field(value: &str) -> bool {
    matches!(
        value,
        "name"
            | "protocol"
            | "baseUrl"
            | "credential"
            | "enabled"
            | "priority"
            | "modelsEndpoint"
            | "probeEndpoint"
            | "probeIntervalMinutes"
            | "slowFirstTokenSeconds"
            | "slowTotalSeconds"
            | "failureThreshold"
            | "recoveryThreshold"
            | "headers"
            | "defaultProbeSettings"
            | "probeSettings"
            | "customProbe"
            | "catalog"
            | "launchAtStartup"
            | "continueInBackground"
            | "trayAction"
            | "theme"
            | "language"
            | "historyRetentionDays"
            | "diagnosticCaptureEnabled"
            | "diagnosticLogMaxMegabytes"
            | "records"
    )
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn rename_if_exists(source: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn read_previous_run(path: &Path) -> Option<PreviousRun> {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<RunMarker>(&contents) {
            Ok(marker) => Some(PreviousRun {
                session_id: Some(marker.session_id),
                started_at: Some(marker.started_at),
            }),
            Err(_) => Some(PreviousRun {
                session_id: None,
                started_at: None,
            }),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(PreviousRun {
            session_id: None,
            started_at: None,
        }),
    }
}

fn sanitize_runtime_token(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
        .then(|| value.to_owned())
}

fn sanitize_source_file(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let source = normalized
        .split_once("/src/")
        .map(|(_, suffix)| format!("src/{suffix}"))
        .or_else(|| normalized.rsplit('/').next().map(ToOwned::to_owned))?;
    if source.is_empty() || source.len() > 200 || source.contains('?') || source.contains('#') {
        return None;
    }
    source
        .chars()
        .all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
        })
        .then_some(source)
}

fn private_append_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn private_create_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn protocol_name(protocol: SiteProtocol) -> &'static str {
    match protocol {
        SiteProtocol::OpenAiCompatible => "openai-compatible",
        SiteProtocol::Anthropic => "anthropic",
        SiteProtocol::OpenAiResponses => "openai-responses",
        SiteProtocol::CustomHttp => "custom-http",
    }
}

fn run_kind_name(run_kind: ProbeRunKind) -> &'static str {
    match run_kind {
        ProbeRunKind::Scheduled => "scheduled",
        ProbeRunKind::Manual => "manual",
        ProbeRunKind::Trial => "trial",
    }
}

fn response_mode_name(mode: ResponseMode) -> &'static str {
    match mode {
        ResponseMode::Stream => "stream",
        ResponseMode::Synchronous => "synchronous",
    }
}

fn startup_error_name(code: StartupErrorCode) -> &'static str {
    match code {
        StartupErrorCode::DataDirectoryUnavailable => "data_directory_unavailable",
        StartupErrorCode::DatabaseSchemaTooNew => "database_schema_too_new",
        StartupErrorCode::DatabaseMigrationFailed => "database_migration_failed",
        StartupErrorCode::DatabaseUnavailable => "database_unavailable",
        StartupErrorCode::CredentialWorkerUnavailable => "credential_worker_unavailable",
    }
}

fn status_name(status: ProbeStatus) -> &'static str {
    match status {
        ProbeStatus::Healthy => "healthy",
        ProbeStatus::Slow => "slow",
        ProbeStatus::Probing => "probing",
        ProbeStatus::RateLimited => "rate-limited",
        ProbeStatus::AuthError => "auth-error",
        ProbeStatus::ModelUnavailable => "model-unavailable",
        ProbeStatus::Timeout => "timeout",
        ProbeStatus::ProtocolError => "protocol-error",
        ProbeStatus::ValidationFailed => "validation-failed",
        ProbeStatus::UpstreamError => "upstream-error",
        ProbeStatus::NetworkError => "network-error",
        ProbeStatus::Cancelled => "cancelled",
        ProbeStatus::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use flate2::read::GzDecoder;

    use super::*;
    use crate::startup::BackendState;

    #[test]
    fn creates_private_structured_log_entries() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        diagnostics.record_state_change(None, 7, &["sites", "records"]);

        let log = fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("log can be read");
        assert!(log.contains("application-started"));
        assert!(log.contains("state-changed"));
        assert!(log.contains("\"revision\":7"));
    }

    #[test]
    fn detects_an_unclean_previous_session_and_removes_the_marker_on_clean_exit() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let first = Diagnostics::initialize(directory.path()).expect("first session starts");
        let first_session_id = first.inner.session_id.clone();
        assert!(directory.path().join(RUN_MARKER_FILE_NAME).exists());
        drop(first);

        let second = Diagnostics::initialize(directory.path()).expect("second session starts");
        let log = fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("log can be read");
        let unexpected = log
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|event| event["event"] == "previous-session-terminated-unexpectedly")
            .expect("unclean session is recorded");
        assert_eq!(unexpected["previousSessionId"], first_session_id);

        second.mark_clean_exit();
        assert!(!directory.path().join(RUN_MARKER_FILE_NAME).exists());
    }

    #[test]
    fn native_panic_metadata_does_not_expose_an_absolute_source_path() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        diagnostics.record_native_panic(
            Some("main"),
            Some("/Users/private/project/src/native/worker.rs"),
            Some(42),
            Some(7),
        );

        let log = fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("log can be read");
        assert!(log.contains(r#""event":"native-panic""#));
        assert!(log.contains(r#""sourceFile":"src/native/worker.rs""#));
        assert!(!log.contains("/Users/private"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exported_package_contains_only_summary_and_safe_log_events() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let state = BackendState::initialize(directory.path().to_path_buf());
        let diagnostics = state.diagnostics().expect("diagnostics initialized");
        diagnostics.record_probe_started(
            "probe_fixture",
            Some("op_fixture"),
            ProbeRunKind::Manual,
            SiteProtocol::OpenAiCompatible,
        );
        diagnostics.record_operation_started(
            "op_audit",
            "update-site",
            Some(&OperationAudit {
                target_type: "site",
                target_id: Some("site_fixture".to_owned()),
                parent_id: None,
                change_kind: "update",
                field_scope: &["baseUrl", "credential"],
            }),
        );
        diagnostics.record_frontend_action(
            "ui_fixture",
            "navigate",
            "sites",
            Some("records"),
            Some("view"),
            Some("records"),
        );
        diagnostics.record_frontend_error(FrontendErrorRecord {
            error_id: "fe_fixture",
            source: "react-render",
            view: "sites",
            error_name: "TypeError",
            fingerprint: "0123456789abcdef",
            stack_frames: &["render@src/App.tsx:42:7".to_owned()],
            last_action_id: Some("ui_fixture"),
        });
        let mut log = OpenOptions::new()
            .append(true)
            .open(directory.path().join("logs/relaypulse.log"))
            .expect("log can be appended");
        writeln!(
            log,
            "{{\"timestamp\":\"now\",\"event\":\"unknown\",\"secret\":\"must-not-export\"}}"
        )
        .expect("untrusted legacy line appended");
        let snapshot = state
            .application_service()
            .expect("application service initialized")
            .snapshot()
            .await
            .expect("snapshot loads");
        let export_path = directory.path().join("diagnostics.json.gz");

        diagnostics
            .export(&export_path, Some(&snapshot), None)
            .expect("diagnostics exported");
        let decoded: Value = serde_json::from_reader(GzDecoder::new(
            File::open(export_path).expect("export can be opened"),
        ))
        .expect("export is valid JSON");

        assert_eq!(decoded["schemaVersion"], 2);
        assert_eq!(decoded["snapshot"]["available"], true);
        assert_eq!(decoded["snapshot"]["siteCount"], 0);
        assert_eq!(
            decoded["storage"]["maxTotalBytes"],
            megabytes_to_bytes(DEFAULT_LOG_TOTAL_MEGABYTES)
        );
        assert_eq!(
            decoded["storage"]["configuredMaxTotalBytes"],
            megabytes_to_bytes(DEFAULT_LOG_TOTAL_MEGABYTES)
        );
        assert!(decoded["logEvents"]
            .as_array()
            .expect("log events are an array")
            .iter()
            .any(|event| {
                event["probeId"] == "probe_fixture" && event["operationId"] == "op_fixture"
            }));
        assert!(decoded["logEvents"]
            .as_array()
            .expect("log events are an array")
            .iter()
            .any(|event| {
                event["operationId"] == "op_audit"
                    && event["targetId"] == "site_fixture"
                    && event["fieldScope"] == json!(["baseUrl", "credential"])
            }));
        assert!(decoded["logEvents"]
            .as_array()
            .expect("log events are an array")
            .iter()
            .any(|event| {
                event["errorId"] == "fe_fixture"
                    && event["lastActionId"] == "ui_fixture"
                    && event["stackFrames"] == json!(["render@src/App.tsx:42:7"])
            }));
        assert!(decoded.get("sites").is_none());
        assert!(!decoded.to_string().contains("must-not-export"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(directory.path().join("diagnostics.json.gz"))
                    .expect("export metadata reads")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn exported_package_remains_available_during_startup_failure() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        let failure = StartupFailure::data_directory();
        diagnostics.record_startup_failed(&failure);
        let export_path = directory.path().join("startup-failure.json.gz");

        diagnostics
            .export(&export_path, None, Some(&failure))
            .expect("diagnostics exported");
        let decoded: Value = serde_json::from_reader(GzDecoder::new(
            File::open(export_path).expect("export can be opened"),
        ))
        .expect("export is valid JSON");

        assert_eq!(decoded["backend"]["ready"], false);
        assert_eq!(
            decoded["backend"]["startupErrorCode"],
            "data_directory_unavailable"
        );
        assert_eq!(decoded["snapshot"]["available"], false);
        assert_eq!(decoded["recentProbeFailures"], json!([]));
        assert!(decoded["logEvents"]
            .as_array()
            .expect("log events are an array")
            .iter()
            .any(|event| event["event"] == "application-startup-failed"));
    }

    #[test]
    fn log_storage_is_bounded_and_can_be_cleared() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        let active = directory.path().join("logs/relaypulse.log");

        for revision in 0..8 {
            fs::write(&active, vec![b'x'; MAX_LOG_FILE_BYTES as usize])
                .expect("oversized active log written");
            diagnostics.record_state_change(None, revision, &["records"]);
        }

        let statistics = diagnostics.statistics().expect("log statistics load");
        assert_eq!(
            statistics.file_count,
            diagnostics.inner.max_rotated_log_files as u64 + 1
        );
        assert!(statistics.total_bytes <= statistics.max_total_bytes);
        for path in diagnostics.log_paths_oldest_first() {
            assert!(
                fs::metadata(path).expect("log metadata reads").len() <= MAX_LOG_FILE_BYTES,
                "each managed log file stays within its hard limit"
            );
        }

        let cleanup = diagnostics.clear_logs().expect("logs clear");
        assert_eq!(cleanup.deleted_files, statistics.file_count);
        assert_eq!(cleanup.reclaimed_bytes, statistics.total_bytes);
        assert_eq!(cleanup.statistics.total_bytes, 0);
        assert_eq!(cleanup.statistics.file_count, 0);
    }

    #[test]
    fn configured_log_limit_is_applied_only_after_restart() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let first = Diagnostics::initialize(directory.path()).expect("diagnostics starts");

        let configured = first
            .configure_max_total_megabytes(37)
            .expect("log limit is configured");
        assert_eq!(
            configured.max_total_bytes,
            megabytes_to_bytes(DEFAULT_LOG_TOTAL_MEGABYTES)
        );
        assert_eq!(
            configured.configured_max_total_bytes,
            megabytes_to_bytes(37)
        );

        let second = Diagnostics::initialize(directory.path()).expect("diagnostics restarts");
        let active = second.statistics().expect("log statistics load");
        assert_eq!(active.max_total_bytes, megabytes_to_bytes(37));
        assert_eq!(active.configured_max_total_bytes, megabytes_to_bytes(37));

        let excess_rotation = directory.path().join("logs/relaypulse.log.5");
        fs::write(&excess_rotation, b"legacy log").expect("legacy rotation written");
        let pending = second
            .configure_max_total_megabytes(10)
            .expect("lower log limit is configured");
        assert_eq!(pending.max_total_bytes, megabytes_to_bytes(37));
        assert_eq!(pending.configured_max_total_bytes, megabytes_to_bytes(10));
        assert!(
            excess_rotation.exists(),
            "running process keeps its active limit"
        );

        let third = Diagnostics::initialize(directory.path()).expect("diagnostics restarts again");
        let lowered = third.statistics().expect("log statistics load");
        assert_eq!(lowered.max_total_bytes, megabytes_to_bytes(10));
        assert_eq!(lowered.configured_max_total_bytes, megabytes_to_bytes(10));
        assert!(
            !excess_rotation.exists(),
            "restart prunes rotations above the new limit"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(directory.path().join(DIAGNOSTIC_SETTINGS_FILE_NAME))
                    .expect("settings metadata reads")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn rejects_log_limits_outside_the_supported_range() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");

        assert_eq!(
            diagnostics
                .configure_max_total_megabytes(9)
                .expect_err("limit below minimum is rejected")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            diagnostics
                .configure_max_total_megabytes(101)
                .expect_err("limit above maximum is rejected")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn non_even_log_limit_remains_a_strict_total_cap() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let first = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        first
            .configure_max_total_megabytes(11)
            .expect("odd log limit is configured");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics restarts");
        let active = directory.path().join("logs/relaypulse.log");

        for revision in 0..10 {
            fs::write(&active, vec![b'x'; MAX_LOG_FILE_BYTES as usize])
                .expect("full active log written");
            diagnostics.record_state_change(None, revision, &["records"]);
        }

        let statistics = diagnostics.statistics().expect("log statistics load");
        assert_eq!(statistics.max_total_bytes, megabytes_to_bytes(11));
        assert!(statistics.total_bytes <= megabytes_to_bytes(11));
    }
}
