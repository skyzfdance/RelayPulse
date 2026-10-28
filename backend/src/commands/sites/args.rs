use crate::modules::sites::*;
use serde::{Deserialize, Deserializer};
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SiteArgs {
    pub site_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateArgs {
    pub input: CreateSite,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SaveArgs {
    pub site_id: String,
    pub draft: SiteDraft,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ConnectionArgs {
    pub site_id: String,
    pub connection: Connection,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReorderArgs {
    pub ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EnabledArgs {
    pub site_id: String,
    pub enabled: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AddArgs {
    pub site_id: String,
    pub name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ModelArgs {
    pub site_id: String,
    pub model_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ModelEnabledArgs {
    pub site_id: String,
    pub model_id: String,
    pub enabled: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SyncArgs {
    pub site_id: String,
    pub models: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ApplyArgs {
    pub site_id: String,
    pub models: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TestArgs {
    pub site_id: String,
    pub model_id: String,
    pub request: CustomRequest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PolicyArgs {
    pub site_id: String,
    #[serde(deserialize_with = "nullable")]
    pub model_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub policy: Option<ProbePolicy>,
    #[serde(deserialize_with = "nullable")]
    pub custom_request: Option<CustomRequest>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RunArgs {
    pub site_id: String,
    pub model_id: Option<String>,
}
