//! 连接正式健康展示数据与 IQ 候选；IQ 的预检和执行完全独立。
use crate::modules::{
    iq_test::{CandidateHealth, IqError},
    monitoring::{Health, Monitoring},
};
use std::{collections::HashMap, sync::Arc};

pub(crate) struct IqCandidateHealth(Arc<Monitoring>);
impl IqCandidateHealth {
    pub(crate) fn new(monitoring: Arc<Monitoring>) -> Self {
        Self(monitoring)
    }
}
impl CandidateHealth for IqCandidateHealth {
    fn read(&self, model_ids: &[String]) -> Result<HashMap<String, String>, IqError> {
        self.0
            .health_snapshot_blocking(model_ids)
            .map(|states| {
                states
                    .into_iter()
                    .map(|(id, state)| {
                        let health = match state.health {
                            Health::Healthy => "healthy",
                            Health::Warning => "warning",
                            Health::Error => "error",
                            Health::Muted => "muted",
                        };
                        (id, health.into())
                    })
                    .collect()
            })
            .map_err(|_| IqError::Storage)
    }
}
