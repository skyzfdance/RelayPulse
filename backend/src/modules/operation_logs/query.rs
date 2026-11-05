use super::{OperationActionGroup, OperationLogError, OperationResult, OperationSource};
use time::{Date, Month};

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OperationLogQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub source: Vec<OperationSource>,
    pub result: Vec<OperationResult>,
    pub actions: Vec<OperationActionGroup>,
    pub page: u64,
    pub page_size: u64,
}
pub(super) fn invalid(field: &'static str, message: &'static str) -> OperationLogError {
    OperationLogError::Validation { field, message }
}
fn date(value: &Option<String>, field: &'static str) -> Result<(), OperationLogError> {
    let Some(value) = value else {
        return Ok(());
    };
    let valid = || -> Option<()> {
        if value.len() != 10
            || value.as_bytes()[4] != b'-'
            || value.as_bytes()[7] != b'-'
            || !value
                .bytes()
                .enumerate()
                .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        {
            return None;
        }
        let year = value.get(..4)?.parse::<i32>().ok()?;
        if !(1..=9999).contains(&year) {
            return None;
        }
        let month = Month::try_from(value.get(5..7)?.parse::<u8>().ok()?).ok()?;
        Date::from_calendar_date(year, month, value.get(8..)?.parse::<u8>().ok()?).ok()?;
        Some(())
    };
    if valid().is_none() {
        return Err(invalid(field, "日期必须为有效的 YYYY-MM-DD 或 null。"));
    }
    Ok(())
}
impl OperationLogQuery {
    pub(crate) fn validate(&self) -> Result<(), OperationLogError> {
        date(&self.start_date, "query.startDate")?;
        date(&self.end_date, "query.endDate")?;
        if self
            .start_date
            .as_ref()
            .zip(self.end_date.as_ref())
            .is_some_and(|(start, end)| start > end)
        {
            return Err(invalid("query.endDate", "结束日期不能早于开始日期。"));
        }
        if self.source.len() > 2 || self.result.len() > 4 || self.actions.len() > 12 {
            return Err(invalid("query", "筛选项数量超出范围。"));
        }
        if self.page == 0 || self.page > 9_007_199_254_740_991 {
            return Err(invalid("query.page", "页码必须是从 1 开始的安全整数。"));
        }
        if !matches!(self.page_size, 10 | 20 | 50 | 100) {
            return Err(invalid(
                "query.pageSize",
                "每页条数必须为 10、20、50 或 100。",
            ));
        }
        Ok(())
    }
}
pub(super) fn log_id(value: &str) -> Result<(), OperationLogError> {
    if value.trim().is_empty() || value.len() > 200 {
        return Err(invalid(
            "logId",
            "操作记录 ID 必须为 1–200 字符的非空白字符串。",
        ));
    }
    Ok(())
}
