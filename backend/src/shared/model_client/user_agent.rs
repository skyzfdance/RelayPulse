//! 默认出站 UA；系统信息只读取一次，不携带主机名、用户或设备标识。
use std::sync::OnceLock;

pub(super) fn value() -> &'static str {
    static VALUE: OnceLock<String> = OnceLock::new();
    VALUE.get_or_init(|| {
        let info = os_info::get();
        let name = match info.os_type() {
            os_info::Type::Macos => "macOS".to_owned(),
            os_info::Type::Unknown => std::env::consts::OS.to_owned(),
            system => system.to_string(),
        };
        let architecture = info.architecture().unwrap_or(std::env::consts::ARCH);
        let architecture = match architecture.to_ascii_lowercase().as_str() {
            "x86" | "x86_64" | "amd64" | "x64" | "i386" | "i486" | "i586" | "i686" => "x86",
            "arm" | "arm64" | "aarch64" | "armv7" | "armv7l" | "armv8" | "armv8l" => "arm",
            _ => architecture,
        };
        format!(
            "RelayPulse/{} ({} {}; {})",
            env!("CARGO_PKG_VERSION"),
            component(&name),
            component(&info.version().to_string()),
            component(architecture),
        )
    })
}

// 系统版本可能含发行版附加文本；规范为空格，避免破坏 UA 注释边界或 HTTP 头。
fn component(value: &str) -> String {
    let safe: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    if safe.is_empty() {
        "unknown".into()
    } else {
        safe
    }
}
