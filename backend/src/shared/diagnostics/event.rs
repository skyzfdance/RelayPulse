//! 诊断字段白名单：只允许技术事件和错误分类，不接受正文、路径、凭据或业务对象。
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    Persistence,
    Configuration,
    Notifications,
    Validation,
    Storage,
    Permission,
    Platform,
    Disabled,
    Network,
    Conflict,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "error",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Outcome {
    Started,
    Succeeded,
    Failed(Failure),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Event {
    pub timestamp_ms: u64,
    // 写入端只接收模块内定义的静态操作标识；读取日志时拥有其字符串。
    pub operation: String,
    pub outcome: Outcome,
}
