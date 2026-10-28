use super::*;
use crate::shared::persistence::{Database, Migration};
use rusqlite::{params, Connection as SqlConnection, OptionalExtension, Transaction};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashSet, sync::Arc};
use zeroize::Zeroizing;

pub(crate) const SCHEMA: Migration = Migration {
    name: "site_configuration",
    sql: include_str!("sites.sql"),
};
#[derive(Clone)]
pub(crate) struct SiteStore {
    database: Arc<Database>,
}
impl SiteStore {
    pub(crate) fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
    fn read<T>(&self, action: impl FnOnce(&SqlConnection) -> Result<T>) -> Result<T> {
        self.database.with_connection(|c| Ok(action(c)))?
    }
    fn write<T>(&self, action: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        self.database.with_connection(|c| {
            let tx = c.transaction()?;
            match action(&tx) {
                Ok(value) => {
                    tx.commit()?;
                    Ok(Ok(value))
                }
                Err(error) => Ok(Err(error)),
            }
        })?
    }
    pub(crate) fn list(&self) -> Result<Vec<Site>> {
        self.read(|c| {
            let ids = c
                .prepare("SELECT id FROM sites ORDER BY priority, id")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.iter().map(|id| load(c, id)).collect()
        })
    }
    pub(crate) fn get(&self, id: &str) -> Result<Site> {
        self.read(|c| load(c, id))
    }
    pub(crate) fn execution_snapshot(&self, id: &str) -> Result<ExecutionSnapshot> {
        self.read(|c| {
            let site = load(c, id)?;
            let secret = c
                .query_row(
                    "SELECT secret FROM site_credentials WHERE site_id=?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or(SiteConfigError::MissingCredential)?;
            let target = crate::shared::model_client::RequestTarget {
                protocol: site.protocol,
                base_url: site.base_url.clone(),
                headers: site.connection.headers.clone(),
                secret: Zeroizing::new(secret),
            };
            Ok(ExecutionSnapshot { site, target })
        })
    }
    pub(crate) fn create(&self, input: &CreateSite, models: &[String]) -> Result<String> {
        let name = validation::name(&input.name)?;
        let url = validation::base_url(&input.base_url)?.to_string();
        validation::credential(&input.secret)?;
        let connection = Connection::for_protocol(input.protocol);
        let id = uuid::Uuid::new_v4().to_string();
        self.write(|tx| {
            tx.execute("INSERT INTO sites(id,name,protocol,base_url,enabled,priority,connection,policy) VALUES (?1,?2,?3,?4,?5,(SELECT COALESCE(MAX(priority),0)+1 FROM sites),?6,?7)",
                params![id,name,protocol_name(input.protocol),url,input.enabled,json(&connection)?,json(&ProbePolicy::for_protocol(input.protocol))?])?;
            tx.execute("INSERT INTO site_credentials VALUES (?1,?2)", params![id,input.secret])?;
            let mut seen = HashSet::new();
            for (index, name) in models.iter().enumerate() {
                if !seen.insert(name) { return Err(SiteConfigError::Duplicate); }
                insert_model(tx, &id, name, index as i64)?;
            }
            load(tx, &id)?;
            Ok(id.clone())
        })
    }
    pub(crate) fn save_basic(&self, id: &str, draft: &SiteDraft) -> Result<Site> {
        let name = validation::name(&draft.name)?;
        let url = validation::base_url(&draft.base_url)?.to_string();
        if draft.credential_action == CredentialAction::Replace {
            validation::credential(&draft.secret)?;
        }
        self.write(|tx| {
            let mut site = load(tx, id)?;
            let protocol_changed = site.protocol != draft.protocol;
            site.protocol = draft.protocol;
            site.base_url = url.clone();
            // 协议/基址改变不能留下无法执行的已保存策略；不暗中重置其他配置。
            validate(&site)?;
            tx.execute("UPDATE sites SET name=?1,protocol=?2,base_url=?3,revision=revision+1 WHERE id=?4", params![name,protocol_name(draft.protocol),url,id])?;
            if protocol_changed {
                // 旧 JSON 已按原协议展开；切换协议也保留用户当前看到的完整参数。
                tx.execute("UPDATE sites SET policy=?1 WHERE id=?2", params![json(&site.policy)?,id])?;
                for model in &site.models {
                    tx.execute("UPDATE site_models SET policy=?1,custom_request=?2 WHERE id=?3", params![model.policy.as_ref().map(json).transpose()?,model.custom_request.as_ref().map(json).transpose()?,model.id])?;
                }
            }
            if draft.credential_action == CredentialAction::Replace {
                tx.execute("INSERT INTO site_credentials VALUES (?1,?2) ON CONFLICT(site_id) DO UPDATE SET secret=excluded.secret", params![id,draft.secret])?;
            }
            load(tx, id)
        })
    }
    pub(crate) fn save_connection(&self, id: &str, value: &Connection) -> Result<Site> {
        self.write(|tx| {
            let site = load(tx, id)?;
            validation::connection(value, &site.base_url)?;
            tx.execute(
                "UPDATE sites SET connection=?1,revision=revision+1 WHERE id=?2",
                params![json(value)?, id],
            )?;
            load(tx, id)
        })
    }
    pub(crate) fn save_policy(
        &self,
        id: &str,
        model_id: Option<&str>,
        policy: Option<&ProbePolicy>,
        custom: Option<&CustomRequest>,
    ) -> Result<Site> {
        self.write(|tx| {
            let site = load(tx, id)?;
            if let Some(value) = policy {
                validation::policy(value, site.protocol)?;
            }
            if let Some(value) = custom {
                validation::custom(value, &site.base_url)?;
            }
            if policy.is_some() && custom.is_some() {
                return Err(invalid(
                    "customRequest",
                    "自定义请求不能与常规策略同时保存。",
                ));
            }
            if let Some(model_id) = model_id {
                require_model(&site, model_id)?;
                tx.execute(
                    "UPDATE site_models SET policy=?1,custom_request=?2 WHERE site_id=?3 AND id=?4",
                    params![
                        policy.map(json).transpose()?,
                        custom.map(json).transpose()?,
                        id,
                        model_id
                    ],
                )?;
            } else {
                if custom.is_some() || policy.is_none() {
                    return Err(invalid(
                        "policy",
                        "站点必须有默认策略，自定义请求仅用于模型。",
                    ));
                }
                tx.execute(
                    "UPDATE sites SET policy=?1 WHERE id=?2",
                    params![json(policy.expect("validated site policy"))?, id],
                )?;
            }
            bump(tx, id)?;
            load(tx, id)
        })
    }
    pub(crate) fn set_enabled(&self, id: &str, enabled: bool) -> Result<Site> {
        self.write(|tx| {
            load(tx, id)?;
            tx.execute(
                "UPDATE sites SET enabled=?1,revision=revision+1 WHERE id=?2",
                params![enabled, id],
            )?;
            load(tx, id)
        })
    }
    pub(crate) fn add_model(&self, id: &str, name: &str) -> Result<Site> {
        let name = validation::name(name)?;
        self.write(|tx| {
            let site = load(tx, id)?;
            if site.models.iter().any(|m| m.name == name) {
                return Err(SiteConfigError::Duplicate);
            }
            let position: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position),-1)+1 FROM site_models WHERE site_id=?1",
                [id],
                |r| r.get(0),
            )?;
            insert_model(tx, id, &name, position)?;
            bump(tx, id)?;
            load(tx, id)
        })
    }
    pub(crate) fn delete_model(&self, id: &str, model_id: &str) -> Result<Site> {
        self.write(|tx| {
            require_model(&load(tx, id)?, model_id)?;
            tx.execute(
                "DELETE FROM site_models WHERE site_id=?1 AND id=?2",
                params![id, model_id],
            )?;
            bump(tx, id)?;
            load(tx, id)
        })
    }
    pub(crate) fn set_model_enabled(
        &self,
        id: &str,
        model_id: &str,
        enabled: bool,
    ) -> Result<Site> {
        self.write(|tx| {
            require_model(&load(tx, id)?, model_id)?;
            tx.execute(
                "UPDATE site_models SET enabled=?1 WHERE site_id=?2 AND id=?3",
                params![enabled, id, model_id],
            )?;
            bump(tx, id)?;
            load(tx, id)
        })
    }
    pub(crate) fn delete(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            let site = load(tx, id)?;
            tx.execute("DELETE FROM sites WHERE id=?1", [id])?;
            tx.execute(
                "UPDATE sites SET priority=priority-1 WHERE priority>?1",
                [site.priority],
            )?;
            Ok(())
        })
    }
    pub(crate) fn reorder(&self, ids: &[String]) -> Result<()> {
        self.write(|tx| {
            let actual = tx
                .prepare("SELECT id FROM sites")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<HashSet<_>>>()?;
            if ids.len() != actual.len()
                || ids.iter().collect::<HashSet<_>>().len() != ids.len()
                || ids.iter().any(|id| !actual.contains(id))
            {
                return Err(SiteConfigError::Stale);
            }
            for (i, id) in ids.iter().enumerate() {
                tx.execute(
                    "UPDATE sites SET priority=?1 WHERE id=?2",
                    params![i as i64 + 1, id],
                )?;
            }
            Ok(())
        })
    }
    pub(crate) fn apply_sync(&self, id: &str, revision: i64, input: &ApplySync) -> Result<()> {
        self.write(|tx| {
            let site = load(tx, id)?;
            if site.revision != revision {
                return Err(SiteConfigError::Stale);
            }
            validation::names_match(&site.models, &input.models)?;
            let mut added = HashSet::new();
            for name in &input.added {
                if validation::name(name)? != *name
                    || !added.insert(name)
                    || site.models.iter().any(|m| m.name == *name)
                {
                    return Err(SiteConfigError::Stale);
                }
            }
            let mut removed = HashSet::new();
            for model in &input.removed {
                require_model(&site, model)?;
                if !removed.insert(model) {
                    return Err(SiteConfigError::Stale);
                }
            }
            for model in &input.removed {
                tx.execute(
                    "DELETE FROM site_models WHERE site_id=?1 AND id=?2",
                    params![id, model],
                )?;
            }
            let position: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position),-1)+1 FROM site_models WHERE site_id=?1",
                [id],
                |r| r.get(0),
            )?;
            for (i, name) in input.added.iter().enumerate() {
                insert_model(tx, id, name, position + i as i64)?;
            }
            bump(tx, id)?;
            load(tx, id)?;
            Ok(())
        })
    }
}
pub(crate) fn require_model<'a>(site: &'a Site, id: &str) -> Result<&'a Model> {
    site.models
        .iter()
        .find(|m| m.id == id)
        .ok_or(SiteConfigError::NotFound)
}
fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|_| SiteConfigError::Storage)
}
fn decode<T: DeserializeOwned>(value: String) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn protocol_name(value: Protocol) -> &'static str {
    match value {
        Protocol::OpenaiCompatible => "openai-compatible",
        Protocol::OpenaiResponses => "openai-responses",
        Protocol::Anthropic => "anthropic",
        Protocol::CustomHttp => "custom-http",
    }
}
fn load(c: &SqlConnection, id: &str) -> Result<Site> {
    let mut site = c.query_row("SELECT id,name,protocol,base_url,enabled,priority,connection,policy,revision,EXISTS(SELECT 1 FROM site_credentials WHERE site_id=sites.id) FROM sites WHERE id=?1", [id], |r| {
        let protocol = decode(format!("\"{}\"",r.get::<_,String>(2)?))?;
        let policy = policy::decode_policy(&r.get::<_,String>(7)?, protocol).map_err(|_|rusqlite::Error::InvalidQuery)?;
        Ok(Site { id:r.get(0)?,name:r.get(1)?,protocol,base_url:r.get(3)?,enabled:r.get(4)?,priority:r.get(5)?,connection:decode(r.get(6)?)?,policy,revision:r.get(8)?,credential_configured:r.get(9)?,models:vec![] })
    }).optional()?.ok_or(SiteConfigError::NotFound)?;
    site.models = c.prepare("SELECT id,name,enabled,policy,custom_request,created_at_ms FROM site_models WHERE site_id=?1 ORDER BY position,id")?.query_map([id], |r| {
        let policy = r.get::<_,Option<String>>(3)?.map(|text|policy::decode_policy(&text, site.protocol)).transpose().map_err(|_|rusqlite::Error::InvalidQuery)?;
        let custom_request = r.get::<_,Option<String>>(4)?.map(|text|policy::decode_custom(&text)).transpose().map_err(|_|rusqlite::Error::InvalidQuery)?;
        Ok(Model {created_at_ms:u64::try_from(r.get::<_,i64>(5)?).map_err(|_|rusqlite::Error::InvalidQuery)?,id:r.get(0)?,name:r.get(1)?,enabled:r.get(2)?,policy,custom_request})
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    validate(&site)?;
    Ok(site)
}
fn validate(site: &Site) -> Result<()> {
    validation::name(&site.name)?;
    validation::base_url(&site.base_url)?;
    validation::connection(&site.connection, &site.base_url)?;
    validation::policy(&site.policy, site.protocol)?;
    for model in &site.models {
        validation::name(&model.name)?;
        if model.policy.is_some() && model.custom_request.is_some() {
            return Err(SiteConfigError::Storage);
        }
        if let Some(policy) = &model.policy {
            validation::policy(policy, site.protocol)?;
        }
        if let Some(custom) = &model.custom_request {
            validation::custom(custom, &site.base_url)?;
        }
    }
    Ok(())
}
fn bump(tx: &Transaction<'_>, id: &str) -> Result<()> {
    tx.execute("UPDATE sites SET revision=revision+1 WHERE id=?1", [id])?;
    Ok(())
}
fn insert_model(tx: &Transaction<'_>, id: &str, name: &str, position: i64) -> Result<()> {
    if validation::name(name)? != name {
        return Err(invalid("name", "模型名称首尾不能有空格。"));
    }
    tx.execute("INSERT INTO site_models(id,site_id,name,position,created_at_ms,enabled) VALUES (?1,?2,?3,?4,?5,1)",params![uuid::Uuid::new_v4().to_string(),id,name,position,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64])?;
    Ok(())
}
