#![allow(non_upper_case_globals)]
use relaypulse_operations::ActionDefinition;
pub const GeneralSettingsUpdated: ActionDefinition =
    ActionDefinition::new("general_settings_updated", "修改通用设置", "update");
pub const PageLoadFailed: ActionDefinition =
    ActionDefinition::new("page_load_failed", "页面加载失败", "read");
