//! 配置继承与执行数据归站点；公共客户端只收到已经解析的请求规格。
use super::{
    access::Lease, require_model, validation, CustomRequest, Model, Protocol, Result, Site,
    SiteError,
};
use crate::shared::model_client::{
    CatalogRequest, CheckBody, CheckRequest, EndpointMode, RequestTarget,
};
use std::sync::Arc;

pub(crate) struct ExecutionSnapshot {
    pub site: Site,
    pub(super) target: RequestTarget,
}
impl ExecutionSnapshot {
    pub(crate) fn resolve_probe_input(&self, model_id: &str) -> Result<CheckRequest> {
        let model = require_model(&self.site, model_id)?;
        if let Some(custom) = &model.custom_request {
            return self.resolve_draft_input(model_id, custom);
        }
        let policy = model.policy.as_ref().unwrap_or(&self.site.policy);
        validation::policy(policy, self.site.protocol)?;
        let body = if self.site.protocol == Protocol::CustomHttp {
            CheckBody::Custom {
                parameters: policy.parameters.clone(),
            }
        } else {
            CheckBody::Standard {
                parameters: policy.parameters.clone(),
            }
        };
        Ok(CheckRequest {
            target: self.target.clone(),
            model: model.name.clone(),
            endpoint: self.site.connection.probe_path.clone(),
            endpoint_mode: EndpointMode::Relative,
            body,
        })
    }
    pub(crate) fn resolve_draft_input(
        &self,
        model_id: &str,
        draft: &CustomRequest,
    ) -> Result<CheckRequest> {
        let model = require_model(&self.site, model_id)?;
        validation::custom(draft, &self.site.base_url)?;
        Ok(CheckRequest {
            target: self.target.clone(),
            model: model.name.clone(),
            endpoint: draft.endpoint.clone(),
            endpoint_mode: EndpointMode::SameOriginAppend,
            body: CheckBody::Custom {
                parameters: draft.parameters.clone(),
            },
        })
    }
    pub(super) fn catalog_request(&self) -> CatalogRequest {
        CatalogRequest {
            target: self.target.clone(),
            endpoint: self.site.connection.models_path.clone(),
        }
    }
}
/// 正式执行在整个请求及结果事务期间持有此作用域；内部锁不暴露给调用者。
pub(crate) struct SiteExecutionScope {
    snapshot: ExecutionSnapshot,
    lease: Arc<Lease>,
}
impl SiteExecutionScope {
    pub(super) fn new(snapshot: ExecutionSnapshot, lease: Arc<Lease>) -> Self {
        Self { snapshot, lease }
    }
    pub(crate) fn site(&self) -> &Site {
        &self.snapshot.site
    }
    pub(crate) fn models(&self) -> &[Model] {
        &self.snapshot.site.models
    }
    pub(crate) fn resolve_probe_input(&self, model_id: &str) -> Result<CheckRequest> {
        self.snapshot.resolve_probe_input(model_id)
    }
    pub(crate) async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> std::result::Result<T, SiteError> {
        self.lease
            .blocking(work)
            .await
            .map_err(|_| SiteError::Worker)
    }
}
