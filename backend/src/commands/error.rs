//! 命令层共享错误契约；各领域的错误映射留在对应命令模块中。
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandError {
    pub(super) code: &'static str,
    pub(super) category: &'static str,
    pub(super) message: String,
    pub(super) field: Option<&'static str>,
    pub(super) retryable: bool,
    pub(super) operation_id: Option<String>,
}

impl CommandError {
    pub(super) fn invalid(field: &'static str, message: &str) -> Self {
        Self {
            code: "invalid_input",
            category: "validation",
            message: message.into(),
            field: Some(field),
            retryable: false,
            operation_id: None,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/commands/error.rs"]
mod tests;
