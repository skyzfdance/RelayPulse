use crate::commands::error::CommandError;
use crate::{
    application::sites::SiteError,
    modules::sites::{SiteConfigError, TaskError},
    shared::model_client::{ClientError, InputError},
};
impl From<SiteError> for CommandError {
    fn from(error: SiteError) -> Self {
        let (code, category, message, retryable) = match error {
            SiteError::Config(SiteConfigError::Invalid { field, message })
            | SiteError::Http(ClientError::Invalid(InputError { field, message })) => {
                return Self::invalid(field, message)
            }
            SiteError::Config(SiteConfigError::NotFound) => (
                "site_not_found",
                "validation",
                "站点或模型不存在，请刷新后重试。",
                false,
            ),
            SiteError::Config(SiteConfigError::Stale) => (
                "site_changed",
                "conflict",
                "站点配置或同步检查结果已变化，请刷新后重新检查。",
                false,
            ),
            SiteError::Config(SiteConfigError::Duplicate) => (
                "site_model_duplicate",
                "validation",
                "此站点已存在同名模型。",
                false,
            ),
            SiteError::Config(SiteConfigError::MissingCredential) => (
                "site_credential_missing",
                "validation",
                "请先配置站点访问凭据。",
                false,
            ),
            SiteError::Task(TaskError::Busy) => (
                "site_busy",
                "conflict",
                "该站点正在执行其他任务，请稍后重试。",
                true,
            ),
            SiteError::Task(TaskError::Closing) => (
                "site_task_cancelled",
                "cancelled",
                "应用正在退出，任务已取消。",
                false,
            ),
            SiteError::Http(ClientError::Timeout) => (
                "site_request_timeout",
                "network",
                "远端目录请求超时，请稍后重试。",
                true,
            ),
            SiteError::Http(ClientError::Status(401 | 403)) => (
                "site_authentication_failed",
                "network",
                "远端拒绝访问，请检查凭据和权限。",
                false,
            ),
            SiteError::Http(ClientError::Status(429)) => (
                "site_rate_limited",
                "network",
                "远端请求受到限流，请稍后重试。",
                true,
            ),
            SiteError::Http(ClientError::Status(_)) => (
                "site_http_failed",
                "network",
                "远端返回非成功状态，请检查目录端点。",
                true,
            ),
            SiteError::Http(ClientError::Response) => (
                "site_catalog_invalid",
                "network",
                "远端模型目录格式无效或超出大小限制。",
                false,
            ),
            SiteError::Http(ClientError::Network | ClientError::Cancelled) => (
                "site_network_failed",
                "network",
                "无法读取远端模型目录，请检查连接配置。",
                true,
            ),
            SiteError::Capture | SiteError::Http(ClientError::Capture) => (
                "probe_capture_failed",
                "storage",
                "本次探测的记录文件保存失败，请检查本地存储后重试。",
                true,
            ),
            SiteError::Worker => (
                "site_task_failed",
                "unavailable",
                "站点任务执行失败，请重试。",
                true,
            ),
            SiteError::Storage | SiteError::Config(SiteConfigError::Storage) => (
                "database_unavailable",
                "storage",
                "无法读写本地站点数据，请检查后重试。",
                true,
            ),
        };
        Self {
            code,
            category,
            message: message.into(),
            field: None,
            retryable,
            operation_id: None,
        }
    }
}
