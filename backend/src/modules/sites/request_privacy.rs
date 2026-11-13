//! 站点请求头的敏感字段规则，供站点请求的记录与 IQ 快照使用。
pub(crate) fn sensitive_request_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace(['-', '_'], "");
    [
        "authorization",
        "key",
        "token",
        "secret",
        "password",
        "credential",
        "cookie",
    ]
    .iter()
    .any(|part| name.contains(part))
        || matches!(name.as_str(), "cookie" | "setcookie" | "xauth")
}
