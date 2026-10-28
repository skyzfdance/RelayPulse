use super::super::sqlite::sql_integer;
use super::CompletedRecord;
use crate::shared::persistence::{Database, PersistenceError};
use rusqlite::{named_params, Transaction};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone)]
pub(in crate::modules::monitoring) struct RecordStore {
    pub(super) database: Arc<Database>,
}
impl RecordStore {
    pub(in crate::modules::monitoring) fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
    pub(in crate::modules::monitoring) fn retained_ids(
        &self,
    ) -> Result<HashSet<String>, PersistenceError> {
        self.database.with_connection(|connection| {
            connection
                .prepare("SELECT id FROM probe_records")?
                .query_map([], |row| row.get(0))?
                .collect()
        })
    }
    pub(in crate::modules::monitoring) fn prune(
        &self,
        cutoff: u64,
    ) -> Result<(), PersistenceError> {
        self.database.with_connection(|connection| {
            let tx = connection.transaction()?;
            prune(&tx, cutoff)?;
            tx.commit()
        })
    }
}

/// 由 monitoring 的同一事务调用；历史存储不读写 probe_states。
pub(in crate::modules::monitoring) fn insert(
    tx: &Transaction<'_>,
    record: &CompletedRecord,
) -> rusqlite::Result<()> {
    let c = &record.context;
    let result = &record.result;
    let metadata = &result.metadata;
    let failure = result
        .failure
        .map(|failure| serde_json::to_string(&failure).expect("unit enum serializes"));
    tx.execute(
        "INSERT INTO probe_records (
            id,site_id,model_id,run_kind,success,status_code,first_token_ms,duration_ms,failure,finished_at_ms,
            site_name,model_name,protocol,started_at_ms,request_method,request_endpoint,policy_json,
            slow_first_token_ms,slow_total_ms,result_kind,request_id,input_tokens,output_tokens,total_tokens,
            response_kind,received_bytes,body_complete,capture_enabled
        ) VALUES (
            :id,:site,:model,:source,:success,:status,:first,:duration,:failure,:finished,
            :site_name,:model_name,:protocol,:started,:method,:endpoint,:policy,
            :slow_first,:slow_total,:outcome,:request_id,:input,:output,:total,
            :response_kind,:bytes,:complete,:capture
        )",
        named_params! {
            ":id":c.id, ":site":c.site_id, ":model":c.model_id, ":source":c.run_kind,
            ":success":result.success, ":status":result.status_code,
            ":first":result.first_token_ms.map(sql_integer).transpose()?,
            ":duration":sql_integer(result.duration_ms)?, ":failure":failure,
            ":finished":sql_integer(record.finished_at_ms)?,
            ":site_name":c.site_name, ":model_name":c.model_name,
            ":protocol":enum_value(c.protocol), ":started":sql_integer(c.started_at_ms)?,
            ":method":c.method, ":endpoint":c.endpoint, ":policy":c.policy.to_string(),
            ":slow_first":sql_integer(c.slow_first_token_ms)?, ":slow_total":sql_integer(c.slow_total_ms)?,
            ":outcome":record.outcome, ":request_id":metadata.request_id,
            ":input":metadata.usage.input_tokens.map(sql_integer).transpose()?,
            ":output":metadata.usage.output_tokens.map(sql_integer).transpose()?,
            ":total":metadata.usage.total_tokens.map(sql_integer).transpose()?,
            ":response_kind":metadata.kind.map(enum_value),
            ":bytes":sql_integer(metadata.received_bytes)?, ":complete":metadata.body_complete,
            ":capture":c.capture_enabled,
        },
    )?;
    Ok(())
}

pub(in crate::modules::monitoring) fn prune(
    tx: &Transaction<'_>,
    cutoff: u64,
) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM probe_records WHERE finished_at_ms<?1",
        [sql_integer(cutoff)?],
    )?;
    Ok(())
}

fn enum_value(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .expect("unit enum serializes")
        .as_str()
        .expect("unit enum serializes to a string")
        .to_owned()
}
