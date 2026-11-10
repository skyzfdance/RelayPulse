//! 诊断字段白名单：只允许技术事件和错误分类，不接受正文、路径、凭据或业务对象。
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Failure(std::borrow::Cow<'static, str>);
#[allow(non_upper_case_globals)]
impl Failure {
    pub const fn code(code: &'static str) -> Self {
        Self(std::borrow::Cow::Borrowed(code))
    }
    pub const Persistence: Self = Self::code("persistence");
    pub const Validation: Self = Self::code("validation");
    pub const Storage: Self = Self::code("storage");
    pub const Permission: Self = Self::code("permission");
    pub const Platform: Self = Self::code("platform");
    pub const Disabled: Self = Self::code("disabled");
    pub const Network: Self = Self::code("network");
    pub const Conflict: Self = Self::code("conflict");
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "error",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Outcome {
    Started,
    Succeeded,
    Failed(Failure),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Event {
    #[serde(default = "event_version")]
    pub format_version: u8,
    pub timestamp_ms: u64,
    // 写入端只接收模块内定义的静态操作标识；读取日志时拥有其字符串。
    pub operation: String,
    pub outcome: Outcome,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}
impl TryFrom<String> for Failure {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !identifier(&value) {
            return Err("invalid diagnostic category");
        }
        Ok(Self(std::borrow::Cow::Owned(value)))
    }
}
impl From<Failure> for String {
    fn from(value: Failure) -> Self {
        value.0.into_owned()
    }
}
impl Event {
    pub(super) fn valid(&self) -> bool {
        self.format_version == 1
            && !self.operation.is_empty()
            && self.operation.len() <= 200
            && self
                .operation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    }
}
fn event_version() -> u8 {
    1
}
