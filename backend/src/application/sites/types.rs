use crate::{
    modules::{
        monitoring::{summarize, Health, HealthState},
        sites::*,
    },
    shared::model_client::CheckResult,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManagedSite {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub enabled: bool,
    pub credential_configured: bool,
    pub connection: Connection,
    pub policy: ProbePolicy,
    pub models: Vec<ModelView>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelView {
    #[serde(flatten)]
    model: Model,
    health: Health,
    first_token_ms: Option<u64>,
    duration_ms: Option<u64>,
}
impl ManagedSite {
    pub(super) fn new(site: Site, states: HashMap<String, HealthState>) -> Self {
        Self {
            id: site.id,
            name: site.name,
            protocol: site.protocol,
            base_url: site.base_url,
            enabled: site.enabled,
            credential_configured: site.credential_configured,
            connection: site.connection,
            policy: site.policy,
            models: site
                .models
                .into_iter()
                .map(|model| {
                    let state = states.get(&model.id).cloned().unwrap_or_default();
                    ModelView {
                        model,
                        health: state.health,
                        first_token_ms: state.first_token_ms,
                        duration_ms: state.duration_ms,
                    }
                })
                .collect(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiteListItem {
    id: String,
    name: String,
    priority: u32,
    enabled: bool,
    model_count: usize,
    health: Health,
    status_text: String,
}
impl SiteListItem {
    pub(super) fn new(site: &Site, states: &HashMap<String, HealthState>) -> Self {
        let summary = summarize(
            site.models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| states.get(&m.id).map_or(Health::Muted, |s| s.health)),
        );
        Self {
            id: site.id.clone(),
            name: site.name.clone(),
            priority: site.priority,
            enabled: site.enabled,
            model_count: site.models.len(),
            health: summary.health,
            status_text: summary.status_text,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestResult {
    success: bool,
    status_code: Option<u16>,
    duration_ms: u64,
    message: &'static str,
    simulated: bool,
}
impl From<CheckResult> for TestResult {
    fn from(value: CheckResult) -> Self {
        Self {
            success: value.success,
            status_code: value.status_code,
            duration_ms: value.duration_ms,
            message: value.message(),
            simulated: false,
        }
    }
}
#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum Reply {
    List(Vec<SiteListItem>),
    Site(Box<ManagedSite>),
    Item(SiteListItem),
    Id(String),
    Changes(SyncChanges),
    Test(TestResult),
    Empty,
}
pub(crate) enum Operation {
    List,
    Get(String),
    Create(CreateSite),
    Save(String, SiteDraft),
    Connection(String, Connection),
    Delete(String),
    Reorder(Vec<String>),
    Enabled(String, bool),
    AddModel(String, String),
    DeleteModel(String, String),
    ModelEnabled(String, String, bool),
    Sync(String, Vec<String>),
    Apply(String, ApplySync),
    Test(String, String, CustomRequest),
    Policy(
        String,
        Option<String>,
        Option<ProbePolicy>,
        Option<CustomRequest>,
    ),
    Run(String, Option<String>),
}
impl Operation {
    pub(super) fn site_id(&self) -> Option<&str> {
        match self {
            Self::List | Self::Reorder(_) => None,
            Self::Create(_) => Some("__create_site__"),
            Self::Get(id)
            | Self::Save(id, _)
            | Self::Connection(id, _)
            | Self::Delete(id)
            | Self::Enabled(id, _)
            | Self::AddModel(id, _)
            | Self::DeleteModel(id, _)
            | Self::ModelEnabled(id, _, _)
            | Self::Sync(id, _)
            | Self::Apply(id, _)
            | Self::Test(id, _, _)
            | Self::Policy(id, _, _, _)
            | Self::Run(id, _) => Some(id),
        }
    }
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::List => "site_list",
            Self::Get(_) => "site_load",
            Self::Create(_) => "site_create",
            Self::Save(..) => "site_save",
            Self::Connection(..) => "site_connection",
            Self::Delete(_) => "site_delete",
            Self::Reorder(_) => "site_reorder",
            Self::Enabled(..) => "site_enabled",
            Self::AddModel(..) => "site_model_add",
            Self::DeleteModel(..) => "site_model_delete",
            Self::ModelEnabled(..) => "site_model_enabled",
            Self::Sync(..) => "site_sync_check",
            Self::Apply(..) => "site_sync_apply",
            Self::Test(..) => "site_probe_test",
            Self::Policy(..) => "site_policy",
            Self::Run(..) => "site_probe_run",
        }
    }
}
