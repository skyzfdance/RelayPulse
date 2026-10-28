use super::error::{RecordError, Result};

#[derive(Clone, Debug)]
pub(crate) struct ProbeRecordsQuery {
    pub range: String,
    pub site_id: Option<String>,
    pub model_name: Option<String>,
    pub result: Option<String>,
    pub page: u64,
    pub page_size: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct TokenStatsQuery {
    pub range: String,
    pub site_id: Option<String>,
    pub page: u64,
    pub page_size: u64,
}
#[derive(Clone, Debug)]
pub(super) struct RecordFilter {
    pub retention_start: u64,
    pub range_start: u64,
    pub now: u64,
    pub site_id: Option<String>,
    pub model_name: Option<String>,
    pub outcome: Option<&'static str>,
    pub page: u64,
    pub page_size: u64,
}
pub(super) fn invalid(field: &'static str, message: &'static str) -> RecordError {
    RecordError::Validation { field, message }
}
pub(super) fn record_id(id: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(invalid("recordId", "记录 ID 必须是非空白字符串。"));
    }
    Ok(())
}
fn filter(value: &Option<String>, field: &'static str) -> Result<()> {
    if value
        .as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.chars().count() > 200)
    {
        return Err(invalid(field, "筛选值必须是 1–200 字符的非空白字符串。"));
    }
    Ok(())
}
fn pagination(page: u64, page_size: u64) -> Result<()> {
    if page == 0 || page > 9_007_199_254_740_991 {
        return Err(invalid("query.page", "页码必须是从 1 开始的安全整数。"));
    }
    if !matches!(page_size, 10 | 20 | 50 | 100) {
        return Err(invalid("query.pageSize", "每页数只允许 10、20、50、100。"));
    }
    Ok(())
}
impl ProbeRecordsQuery {
    pub(super) fn validate(&self) -> Result<()> {
        range(&self.range, 0)?;
        filter(&self.site_id, "query.siteId")?;
        filter(&self.model_name, "query.modelName")?;
        outcome(self.result.as_deref())?;
        pagination(self.page, self.page_size)
    }
    pub(super) fn at(self, now: u64, retention_days: u16) -> Result<RecordFilter> {
        Ok(RecordFilter {
            retention_start: now.saturating_sub(u64::from(retention_days) * 86_400_000),
            range_start: range(&self.range, now)?,
            now,
            site_id: self.site_id,
            model_name: self.model_name,
            outcome: outcome(self.result.as_deref())?,
            page: self.page,
            page_size: self.page_size,
        })
    }
}
impl TokenStatsQuery {
    pub(super) fn records(self) -> ProbeRecordsQuery {
        ProbeRecordsQuery {
            range: self.range,
            site_id: self.site_id,
            model_name: None,
            result: None,
            page: self.page,
            page_size: self.page_size,
        }
    }
}
fn range(value: &str, now: u64) -> Result<u64> {
    let days = match value {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "all" => return Ok(0),
        _ => return Err(invalid("query.range", "时间范围不合法。")),
    };
    Ok(now.saturating_sub(days * 86_400_000))
}
fn outcome(value: Option<&str>) -> Result<Option<&'static str>> {
    Ok(match value {
        None => None,
        Some("success") => Some("succeeded"),
        Some("slow") => Some("slow"),
        Some("failure") => Some("failed"),
        _ => return Err(invalid("query.result", "请求结果筛选不合法。")),
    })
}
pub(super) fn actual_page(requested: u64, total: u64, size: u64) -> u64 {
    requested.min(total.saturating_sub(1) / size + 1)
}
