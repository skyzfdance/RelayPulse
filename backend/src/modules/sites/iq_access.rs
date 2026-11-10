//! IQ 专属的内部凭据授权。与普通 DTO 读取、站点执行互斥分别建模。
use super::{require_model, Model, Protocol, Result, SiteConfigError, SiteService, SiteStore};
use crate::shared::model_client::RequestTarget;

/// 只能由 sites 发放；composition 将此能力交给 IQ，不注册为前端命令。
#[derive(Clone)]
pub struct IqCredentialAccess {
    store: SiteStore,
}
#[derive(Clone)]
pub struct IqModelSource {
    pub model_key: String,
    pub model_name: String,
    pub site_id: String,
    pub site_name: String,
    pub protocol: Protocol,
}
/// 不实现 Serialize/Debug；连接目标含实际密钥，只允许留在内存执行上下文中。
#[derive(Clone)]
pub struct IqConnectionSnapshot {
    pub source: IqModelSource,
    pub target: RequestTarget,
    pub endpoint: String,
}
impl SiteService {
    pub fn authorize_iq_credentials(&self) -> IqCredentialAccess {
        IqCredentialAccess {
            store: self.store.clone(),
        }
    }
}
impl IqCredentialAccess {
    /// 调用方在阻塞 worker 中执行。站点开关不参与候选筛选。
    pub fn candidates(&self) -> Result<Vec<IqModelSource>> {
        Ok(self
            .store
            .list()?
            .into_iter()
            .flat_map(|site| {
                site.models
                    .iter()
                    .filter(|model| model.enabled)
                    .map(|model| source(&site, model))
                    .collect::<Vec<_>>()
            })
            .collect())
    }
    pub fn snapshot(&self, model_key: &str) -> Result<IqConnectionSnapshot> {
        let (site_id, model_id) = model_key.split_once('/').ok_or(SiteConfigError::NotFound)?;
        // 一次数据库锁内读取站点、模型和凭据，不持有 SiteAccess 的执行 lease。
        let snapshot = self.store.iq_connection_snapshot(site_id)?;
        let model = require_model(&snapshot.site, model_id)?;
        if !model.enabled {
            return Err(SiteConfigError::NotFound);
        }
        Ok(IqConnectionSnapshot {
            source: source(&snapshot.site, model),
            endpoint: snapshot.site.connection.probe_path.clone(),
            target: snapshot.target,
        })
    }
}
fn source(site: &super::Site, model: &Model) -> IqModelSource {
    IqModelSource {
        model_key: format!("{}/{}", site.id, model.id),
        model_name: model.name.clone(),
        site_id: site.id.clone(),
        site_name: site.name.clone(),
        protocol: site.protocol,
    }
}
