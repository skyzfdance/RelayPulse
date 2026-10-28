use crate::modules::configuration::ThemeMode;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelSnapshot {
    pub revision: u64,
    pub theme: ThemeMode,
    pub state: PanelState,
    pub site_count: usize,
    pub model_count: usize,
    pub sites: Vec<PanelSiteGroup>,
    pub last_probe: Option<PanelProbeCompletion>,
    pub active_probe: Option<PanelProbeProgress>,
    pub schedule: PanelSchedule,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PanelState {
    Waiting,
    Ready,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelSiteGroup {
    pub site_id: String,
    pub site_name: String,
    pub models: Vec<PanelModelResult>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelModelResult {
    pub id: String,
    pub model_name: String,
    pub status: PanelModelStatus,
    pub first_token_ms: Option<u64>,
    pub total_ms: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PanelModelStatus {
    Healthy,
    Unavailable,
    Slow,
    Pending,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelProbeCompletion {
    pub completed_at: String,
    pub completed_count: usize,
    pub total_count: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelProbeProgress {
    pub completed_count: usize,
    pub total_count: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub(crate) enum PanelSchedule {
    Disabled,
    Scheduled {
        #[serde(rename = "nextProbeAt")]
        next_probe_at: String,
    },
}
