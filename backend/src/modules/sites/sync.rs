use super::{Model, RemovedModel, SiteError, SyncChanges};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
#[derive(Clone)]
pub(super) struct PendingSync {
    pub revision: i64,
    pub changes: SyncChanges,
}
#[derive(Default)]
pub(super) struct SyncChecks(Mutex<HashMap<String, PendingSync>>);
impl SyncChecks {
    pub fn forget(&self, id: &str) -> Result<(), SiteError> {
        self.0.lock().map_err(|_| SiteError::Worker)?.remove(id);
        Ok(())
    }
    pub fn put(&self, id: String, check: PendingSync) -> Result<(), SiteError> {
        self.0
            .lock()
            .map_err(|_| SiteError::Worker)?
            .insert(id, check);
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<PendingSync, SiteError> {
        self.0
            .lock()
            .map_err(|_| SiteError::Worker)?
            .get(id)
            .cloned()
            .ok_or_else(|| super::SiteConfigError::Stale.into())
    }
}
pub(super) fn same_set(a: &[String], b: &[String]) -> bool {
    let unique: HashSet<_> = a.iter().collect();
    a.len() == b.len() && unique.len() == a.len() && b.iter().all(|v| unique.contains(v))
}
pub(crate) fn differences(local: &[Model], remote: &[String]) -> SyncChanges {
    let existing: HashSet<_> = local.iter().map(|m| m.name.as_str()).collect();
    let incoming: HashSet<_> = remote.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    SyncChanges {
        added: remote
            .iter()
            .filter(|name| !existing.contains(name.as_str()) && seen.insert(name.as_str()))
            .cloned()
            .collect(),
        removed: local
            .iter()
            .filter(|m| !incoming.contains(m.name.as_str()))
            .map(|m| RemovedModel {
                id: m.id.clone(),
                name: m.name.clone(),
            })
            .collect(),
    }
}
