//! 只查询/删除既有 probe_records；不访问当前健康、配置或凭据表。
use super::super::sqlite::sql_integer;
use super::{
    dto::*,
    query::{actual_page, RecordFilter},
    RecordStore,
};
use crate::shared::{model_client::TokenUsage, persistence::PersistenceError};
use rusqlite::{named_params, OptionalExtension, Row, RowIndex};
use serde_json::Value;

// 保留期沿用完成时间，页面时间和稳定顺序使用请求开始时间；旧快照缺失时只回退到历史完成时间。
const HISTORY: &str = "WITH retained AS (
    SELECT *, COALESCE(started_at_ms,finished_at_ms) AS probed_at_ms,
        CASE WHEN success=0 THEN 'failed'
             WHEN result_kind IS NOT NULL THEN result_kind
             WHEN duration_ms>=slow_total_ms OR first_token_ms>=slow_first_token_ms THEN 'slow'
             ELSE 'succeeded' END AS outcome
    FROM probe_records WHERE finished_at_ms>=:retention AND finished_at_ms<=:now
        AND COALESCE(started_at_ms,finished_at_ms)<=:now
), filtered AS (
    SELECT * FROM retained WHERE probed_at_ms>=:since
        AND (:site IS NULL OR site_id=:site)
        AND (:model IS NULL OR model_name=:model)
        AND (:outcome IS NULL OR outcome=:outcome)
)";
const TOTALS: &str = "COUNT(*) AS probe_count,
    COUNT(CASE WHEN input_tokens IS NOT NULL OR output_tokens IS NOT NULL OR total_tokens IS NOT NULL THEN 1 END) AS reported_count,
    COUNT(total_tokens) AS total_reported_count,
    SUM(input_tokens) AS input_tokens, SUM(output_tokens) AS output_tokens,
    SUM(total_tokens) AS total_tokens,
    CAST(SUM(total_tokens) AS REAL)/NULLIF(COUNT(total_tokens),0) AS average_tokens";

pub(super) struct StoredRecord {
    pub id: String,
    pub site_name: Option<String>,
    pub model_name: Option<String>,
    pub probed_at_ms: u64,
    pub outcome: String,
    pub status_code: Option<u16>,
    pub first_token_ms: Option<u64>,
    pub duration_ms: u64,
    pub failure: Option<String>,
    pub method: Option<String>,
    pub endpoint: Option<String>,
    pub policy: Option<Value>,
    pub protocol: Option<String>,
    pub slow_first_token_ms: Option<u64>,
    pub slow_total_ms: Option<u64>,
    pub request_id: Option<String>,
    pub usage: TokenUsage,
    pub response_kind: Option<String>,
    pub received_bytes: Option<u64>,
    pub body_complete: Option<bool>,
    pub capture_enabled: Option<bool>,
}
pub(super) struct RecordRows {
    pub items: Vec<StoredRecord>,
    pub total: u64,
    pub page: u64,
    pub retained_count: u64,
}

macro_rules! filters {
    ($f:expr) => { named_params! {
        ":retention":sql_integer($f.retention_start)?, ":now":sql_integer($f.now)?, ":since":sql_integer($f.range_start)?,
        ":site":$f.site_id, ":model":$f.model_name, ":outcome":$f.outcome,
    } };
}
impl RecordStore {
    pub(super) fn list(&self, f: RecordFilter) -> Result<RecordRows, PersistenceError> {
        self.database.with_connection(|c| {
            let (total, retained_count): (u64,u64) = c.query_row(
                &format!("{HISTORY} SELECT COUNT(*),(SELECT COUNT(*) FROM retained) FROM filtered"),
                filters!(f), |r| Ok((number(r,0)?,number(r,1)?)))?;
            let page = actual_page(f.page, total, f.page_size);
            let mut statement = c.prepare(&format!("{HISTORY} SELECT * FROM filtered
                ORDER BY probed_at_ms DESC,id ASC LIMIT :limit OFFSET :offset"))?;
            let items = statement.query_map(named_params! {
                ":retention":sql_integer(f.retention_start)?, ":now":sql_integer(f.now)?, ":since":sql_integer(f.range_start)?,
                ":site":f.site_id, ":model":f.model_name, ":outcome":f.outcome,
                ":limit":sql_integer(f.page_size)?, ":offset":sql_integer((page-1)*f.page_size)?,
            }, stored)?.collect::<rusqlite::Result<_>>()?;
            Ok(RecordRows {items,total,page,retained_count})
        })
    }
    pub(super) fn token_stats(&self, f: RecordFilter) -> Result<TokenStatsPage, PersistenceError> {
        self.database.with_connection(|c| {
            let (total, retained_count): (u64,u64) = c.query_row(
                &format!("{HISTORY} SELECT COUNT(DISTINCT site_id),(SELECT COUNT(*) FROM retained) FROM filtered"),
                filters!(f), |r| Ok((number(r,0)?,number(r,1)?)))?;
            let summary = c.query_row(&format!("{HISTORY} SELECT {TOTALS} FROM filtered"), filters!(f), totals)?;
            let page = actual_page(f.page, total, f.page_size);
            let mut statement = c.prepare(&format!("{HISTORY}, ranked AS (
                SELECT *, ROW_NUMBER() OVER (PARTITION BY site_id ORDER BY probed_at_ms DESC,id ASC) AS position
                FROM filtered
            ) SELECT site_id,MAX(CASE WHEN position=1 THEN site_name END) AS latest_name,{TOTALS}
                FROM ranked GROUP BY site_id
                ORDER BY total_tokens IS NULL ASC,total_tokens DESC,site_id ASC
                LIMIT :limit OFFSET :offset"))?;
            let items = statement.query_map(named_params! {
                ":retention":sql_integer(f.retention_start)?, ":now":sql_integer(f.now)?, ":since":sql_integer(f.range_start)?,
                ":site":f.site_id, ":model":f.model_name, ":outcome":f.outcome,
                ":limit":sql_integer(f.page_size)?, ":offset":sql_integer((page-1)*f.page_size)?,
            }, |r| Ok(SiteTokenStats { site_id:r.get("site_id")?,
                site_name:r.get::<_,Option<String>>("latest_name")?.unwrap_or_else(|| "未知站点".into()),
                usage:totals(r)?, }))?.collect::<rusqlite::Result<_>>()?;
            Ok(TokenStatsPage { records:RecordsPage {items,total,page,retained_count},summary })
        })
    }
    pub(super) fn get(
        &self,
        id: &str,
        retention_start: u64,
        now: u64,
    ) -> Result<Option<StoredRecord>, PersistenceError> {
        self.database.with_connection(|c| c.query_row(
            &format!("{HISTORY} SELECT * FROM filtered WHERE id=:id"),
            named_params! {":retention":sql_integer(retention_start)?, ":now":sql_integer(now)?, ":since":0,
                ":site":Option::<String>::None, ":model":Option::<String>::None,
                ":outcome":Option::<String>::None, ":id":id}, stored).optional())
    }
    pub(super) fn delete(&self, id: &str) -> Result<(), PersistenceError> {
        self.database.with_connection(|c| {
            c.execute("DELETE FROM probe_records WHERE id=?1", [id])
                .map(|_| ())
        })
    }
    pub(super) fn clear(&self) -> Result<(), PersistenceError> {
        self.database
            .with_connection(|c| c.execute("DELETE FROM probe_records", []).map(|_| ()))
    }
    pub(super) fn has_captures(&self) -> Result<bool, PersistenceError> {
        self.database.with_connection(|c| {
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM probe_records WHERE capture_enabled=1)",
                [],
                |r| r.get(0),
            )
        })
    }
}
fn totals(r: &Row<'_>) -> rusqlite::Result<TokenStatsSummary> {
    Ok(TokenStatsSummary {
        probe_count: number(r, "probe_count")?,
        reported_count: number(r, "reported_count")?,
        total_reported_count: number(r, "total_reported_count")?,
        input_tokens: optional_number(r, "input_tokens")?,
        output_tokens: optional_number(r, "output_tokens")?,
        total_tokens: optional_number(r, "total_tokens")?,
        average_tokens: r.get("average_tokens")?,
    })
}
fn stored(r: &Row<'_>) -> rusqlite::Result<StoredRecord> {
    Ok(StoredRecord {
        id: r.get("id")?,
        site_name: r.get("site_name")?,
        model_name: r.get("model_name")?,
        probed_at_ms: number(r, "probed_at_ms")?,
        outcome: r.get("outcome")?,
        status_code: r.get("status_code")?,
        first_token_ms: optional_number(r, "first_token_ms")?,
        duration_ms: number(r, "duration_ms")?,
        failure: r
            .get::<_, Option<String>>("failure")?
            .map(|s| serde_json::from_str(&s).map_err(|_| rusqlite::Error::InvalidQuery))
            .transpose()?,
        method: r.get("request_method")?,
        endpoint: r.get("request_endpoint")?,
        policy: r
            .get::<_, Option<String>>("policy_json")?
            .map(|s| serde_json::from_str(&s).map_err(|_| rusqlite::Error::InvalidQuery))
            .transpose()?,
        protocol: r.get("protocol")?,
        slow_first_token_ms: optional_number(r, "slow_first_token_ms")?,
        slow_total_ms: optional_number(r, "slow_total_ms")?,
        request_id: r.get("request_id")?,
        usage: TokenUsage {
            input_tokens: optional_number(r, "input_tokens")?,
            output_tokens: optional_number(r, "output_tokens")?,
            total_tokens: optional_number(r, "total_tokens")?,
        },
        response_kind: r.get("response_kind")?,
        received_bytes: optional_number(r, "received_bytes")?,
        body_complete: r.get("body_complete")?,
        capture_enabled: r.get("capture_enabled")?,
    })
}
fn unsigned(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn number(r: &Row<'_>, key: impl RowIndex) -> rusqlite::Result<u64> {
    unsigned(r.get(key)?)
}
fn optional_number(r: &Row<'_>, key: impl RowIndex) -> rusqlite::Result<Option<u64>> {
    r.get::<_, Option<i64>>(key)?.map(unsigned).transpose()
}
