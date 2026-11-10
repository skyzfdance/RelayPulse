use super::{
    types::enum_string, GenericHistoryDecoder, HistoryDecoder, OperationLogDetail,
    OperationLogError, OperationLogListItem, OperationLogPage, OperationLogQuery,
};
use crate::shared::persistence::{Database, Migration};
use rusqlite::{params, params_from_iter, types::Value, OptionalExtension, Row};
use std::sync::Arc;

pub const SCHEMA: Migration = Migration {
    name: "operation_history",
    sql: include_str!("operation_logs.sql"),
};
pub(super) struct Store {
    database: Arc<Database>,
    pub(super) decoder: Arc<dyn HistoryDecoder>,
}
impl Store {
    pub(super) fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            decoder: Arc::new(GenericHistoryDecoder),
        }
    }
    pub(super) fn insert(
        &self,
        at: i64,
        detail: &OperationLogDetail,
    ) -> Result<(), OperationLogError> {
        let item = &detail.item;
        let behavior = serde_json::to_string(&item.detail_behavior)
            .map_err(|_| OperationLogError::Unavailable)?;
        let content =
            serde_json::to_string(&detail.content).map_err(|_| OperationLogError::Unavailable)?;
        self.database.with_connection(|c|{
            c.execute("INSERT OR IGNORE INTO operation_logs(id,occurred_at_ms,occurred_at,source,actor_snapshot,action,action_group,action_label,object_snapshot,result,summary,detail_behavior,explanation,content) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![
                item.id,at,item.occurred_at,enum_string(item.source),item.actor_snapshot,enum_string(&item.action),enum_string(&item.action_group),item.action_label,item.object_snapshot,enum_string(item.result),item.summary,behavior,detail.explanation,content])?;
            Ok(())
        }).map_err(|_|OperationLogError::Unavailable)
    }
    pub(super) fn query(
        &self,
        q: OperationLogQuery,
    ) -> Result<OperationLogPage, OperationLogError> {
        let mut clauses = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        // SQLite localtime uses the device timezone for each timestamp (including DST),
        // rather than applying the current offset to historical dates.
        for (op, value) in [(">=", q.start_date.as_ref()), ("<=", q.end_date.as_ref())] {
            if let Some(value) = value {
                clauses.push(format!(
                    "date(occurred_at_ms/1000.0,'unixepoch','localtime') {op} ?"
                ));
                values.push(value.clone().into());
            }
        }
        for (column, selected) in [
            (
                "source",
                q.source.iter().map(enum_string).collect::<Vec<_>>(),
            ),
            (
                "result",
                q.result.iter().map(enum_string).collect::<Vec<_>>(),
            ),
        ] {
            if !selected.is_empty() {
                clauses.push(format!(
                    "{column} IN ({})",
                    vec!["?"; selected.len()].join(",")
                ));
                values.extend(selected.into_iter().map(Value::from));
            }
        }
        if !q.actions.is_empty() {
            let slots = vec!["?"; q.actions.len()].join(",");
            // Resolve old raw-action links through persisted group snapshots, without a catalog.
            clauses.push(format!("(action_group IN ({slots}) OR action_group IN (SELECT action_group FROM operation_logs WHERE action IN ({slots})))"));
            for _ in 0..2 {
                values.extend(q.actions.iter().cloned().map(Value::from));
            }
        }
        let filter = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        self.database.with_connection(|c|{
            let tx=c.transaction()?;
            let total=tx.query_row(&format!("SELECT count(*) FROM operation_logs{filter}"),params_from_iter(values.iter()),|r|r.get::<_,i64>(0))? as u64;
            let page=q.page.min(total.saturating_sub(1)/q.page_size+1);
            let mut arguments=values; arguments.push((q.page_size as i64).into()); arguments.push((((page-1)*q.page_size) as i64).into());
            let items=tx.prepare(&format!("SELECT {LIST_COLUMNS} FROM operation_logs{filter} ORDER BY occurred_at_ms DESC,id ASC LIMIT ? OFFSET ?"))?
                .query_map(params_from_iter(arguments.iter()),|r| list_item(r, self.decoder.as_ref()))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let action_groups=tx.prepare("SELECT DISTINCT action_group FROM operation_logs ORDER BY action_group")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            tx.commit()?;
            Ok(OperationLogPage{items,total,page,page_size:q.page_size,action_groups})
        }).map_err(|_|OperationLogError::Unavailable)
    }
    pub(super) fn detail(&self, id: &str) -> Result<OperationLogDetail, OperationLogError> {
        self.database
            .with_connection(|c| {
                c.query_row(
                    &format!(
                        "SELECT {LIST_COLUMNS},explanation,content FROM operation_logs WHERE id=?1"
                    ),
                    [id],
                    |r| {
                        Ok(OperationLogDetail {
                            item: list_item(r, self.decoder.as_ref())?,
                            explanation: r.get(11)?,
                            content: decode(r, 12, |value| self.decoder.content(value))?,
                        })
                    },
                )
                .optional()
            })
            .map_err(|_| OperationLogError::Unavailable)?
            .ok_or(OperationLogError::NotFound)
    }
}
const LIST_COLUMNS:&str="id,occurred_at,source,actor_snapshot,action,action_group,action_label,object_snapshot,result,summary,detail_behavior";
fn decode<T>(
    r: &Row<'_>,
    index: usize,
    decoder: impl FnOnce(serde_json::Value) -> Result<T, serde_json::Error>,
) -> rusqlite::Result<T> {
    let text: String = r.get(index)?;
    serde_json::from_str(&text).and_then(decoder).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn unit<T: serde::de::DeserializeOwned>(r: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = r.get(index)?;
    serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn list_item(r: &Row<'_>, decoder: &dyn HistoryDecoder) -> rusqlite::Result<OperationLogListItem> {
    Ok(OperationLogListItem {
        id: r.get(0)?,
        occurred_at: r.get(1)?,
        source: unit(r, 2)?,
        actor_snapshot: r.get(3)?,
        action: unit(r, 4)?,
        action_group: unit(r, 5)?,
        action_label: r.get(6)?,
        object_snapshot: r.get(7)?,
        result: unit(r, 8)?,
        summary: r.get(9)?,
        detail_behavior: decode(r, 10, |value| decoder.behavior(value))?,
    })
}
