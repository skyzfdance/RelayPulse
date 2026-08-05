use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use flate2::{write::GzEncoder, Compression};
use serde_json::{json, Value};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    application::ApplicationSnapshot,
    domain::{ProbeStatus, SiteProtocol},
};

const LOG_DIRECTORY_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "relaypulse.log";
const ROTATED_LOG_FILE_NAME: &str = "relaypulse.log.1";
const MAX_LOG_BYTES: u64 = 512 * 1024;
const MAX_EXPORTED_LOG_EVENTS: usize = 500;

#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<DiagnosticsInner>,
}

struct DiagnosticsInner {
    log_directory: PathBuf,
    log_file: PathBuf,
    write_lock: Mutex<()>,
}

impl Diagnostics {
    pub fn initialize(data_directory: &Path) -> io::Result<Self> {
        let log_directory = data_directory.join(LOG_DIRECTORY_NAME);
        fs::create_dir_all(&log_directory)?;
        let diagnostics = Self {
            inner: Arc::new(DiagnosticsInner {
                log_file: log_directory.join(LOG_FILE_NAME),
                log_directory,
                write_lock: Mutex::new(()),
            }),
        };
        diagnostics.record_startup();
        Ok(diagnostics)
    }

    pub fn log_directory(&self) -> &Path {
        &self.inner.log_directory
    }

    pub fn record_startup(&self) {
        self.append(json!({ "event": "application-started" }));
    }

    pub fn record_state_change(&self, revision: u64, domains: &[&str]) {
        self.append(json!({
            "event": "state-changed",
            "revision": revision,
            "domains": domains,
        }));
    }

    pub fn record_records_cleared(&self, deleted: u64, revision: u64) {
        self.append(json!({
            "event": "probe-records-cleared",
            "deleted": deleted,
            "revision": revision,
        }));
    }

    pub fn record_log_directory_opened(&self) {
        self.append(json!({ "event": "log-directory-opened" }));
    }

    pub fn record_diagnostics_exported(&self) {
        self.append(json!({ "event": "diagnostics-exported" }));
    }

    pub fn record_window_opened(&self, window: &'static str) {
        self.append(json!({
            "event": "window-opened",
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

    pub fn export(&self, path: &Path, snapshot: &ApplicationSnapshot) -> io::Result<()> {
        let mut protocols = BTreeMap::<&'static str, usize>::new();
        let mut statuses = BTreeMap::<&'static str, usize>::new();
        let mut model_count = 0usize;
        let mut enabled_model_count = 0usize;
        let mut enabled_site_count = 0usize;
        let mut credential_site_count = 0usize;

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

        let package = json!({
            "schemaVersion": 1,
            "generatedAt": timestamp(),
            "application": {
                "name": "RelayPulse",
                "version": env!("CARGO_PKG_VERSION"),
                "operatingSystem": std::env::consts::OS,
                "architecture": std::env::consts::ARCH,
            },
            "snapshot": {
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
            },
            "logEvents": self.log_events(),
            "exclusions": [
                "API credentials and credential references",
                "SQLite database contents",
                "site names and URLs",
                "configured headers, prompts, parameters, request bodies, and response bodies",
            ],
        });

        let file = File::create(path)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        serde_json::to_writer_pretty(&mut encoder, &package).map_err(io::Error::other)?;
        encoder.finish()?;
        Ok(())
    }

    fn log_events(&self) -> Vec<Value> {
        let Ok(file) = File::open(&self.inner.log_file) else {
            return Vec::new();
        };
        let events = BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
            .filter_map(sanitize_log_event)
            .collect::<Vec<_>>();
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
        let _ = self.rotate_if_needed();
        if let Some(object) = entry.as_object_mut() {
            object.insert("timestamp".to_owned(), Value::String(timestamp()));
        }
        let Ok(mut file) = private_append_file(&self.inner.log_file) else {
            return;
        };
        if serde_json::to_writer(&mut file, &entry).is_ok() {
            let _ = file.write_all(b"\n");
        }
    }

    fn rotate_if_needed(&self) -> io::Result<()> {
        let Ok(metadata) = fs::metadata(&self.inner.log_file) else {
            return Ok(());
        };
        if metadata.len() < MAX_LOG_BYTES {
            return Ok(());
        }
        let rotated = self.inner.log_directory.join(ROTATED_LOG_FILE_NAME);
        if rotated.exists() {
            fs::remove_file(&rotated)?;
        }
        fs::rename(&self.inner.log_file, rotated)
    }
}

fn sanitize_log_event(value: Value) -> Option<Value> {
    let event = value.get("event")?.as_str()?;
    let timestamp = value.get("timestamp")?.as_str()?;
    match event {
        "application-started" | "log-directory-opened" | "diagnostics-exported" => Some(json!({
            "timestamp": timestamp,
            "event": event,
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
                "event": event,
                "revision": revision,
                "domains": domains,
            }))
        }
        "probe-records-cleared" => Some(json!({
            "timestamp": timestamp,
            "event": event,
            "deleted": value.get("deleted")?.as_u64()?,
            "revision": value.get("revision")?.as_u64()?,
        })),
        "autostart-reconciled" => Some(json!({
            "timestamp": timestamp,
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
                "event": event,
                "window": window,
            }))
        }
        _ => None,
    }
}

fn private_append_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
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
        diagnostics.record_state_change(7, &["sites", "records"]);

        let log = fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("log can be read");
        assert!(log.contains("application-started"));
        assert!(log.contains("state-changed"));
        assert!(log.contains("\"revision\":7"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exported_package_contains_only_summary_and_safe_log_events() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let state = BackendState::initialize(directory.path().to_path_buf());
        let diagnostics = state.diagnostics().expect("diagnostics initialized");
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
            .export(&export_path, &snapshot)
            .expect("diagnostics exported");
        let decoded: Value = serde_json::from_reader(GzDecoder::new(
            File::open(export_path).expect("export can be opened"),
        ))
        .expect("export is valid JSON");

        assert_eq!(decoded["schemaVersion"], 1);
        assert_eq!(decoded["snapshot"]["siteCount"], 0);
        assert!(decoded.get("sites").is_none());
        assert!(!decoded.to_string().contains("must-not-export"));
    }
}
