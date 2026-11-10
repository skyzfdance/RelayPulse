use super::*;
use crate::shared::persistence::{Database, Migration};
use rusqlite::{params, types::Type, Connection, OptionalExtension, Transaction};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

pub const SCHEMA: Migration = Migration {
    name: "iq_test",
    sql: include_str!("iq_test.sql"),
};
#[derive(Clone)]
pub(super) struct Store {
    database: Arc<Database>,
}
impl Store {
    pub(super) fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
    fn read<T>(&self, work: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.database
            .with_connection(|connection| Ok(work(connection)))?
    }
    fn write<T>(&self, work: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        self.database.with_connection(|connection| {
            let tx = connection.transaction()?;
            let result = work(&tx);
            if result.is_ok() {
                tx.commit()?;
            }
            Ok(result)
        })?
    }
    pub(super) fn questions(&self) -> Result<Vec<IqQuestion>> {
        self.read(|c| Ok(c.prepare("SELECT id,content,reference_answer,enabled,created_at_ms,updated_at_ms FROM iq_questions ORDER BY enabled DESC,updated_at_ms DESC,id")?
            .query_map([], question)?.collect::<rusqlite::Result<_>>()?))
    }
    pub(super) fn save_question(
        &self,
        id: Option<&str>,
        input: IqQuestionInput,
    ) -> Result<IqQuestion> {
        self.write(|tx| {
            let now = now_ms();
            let created = if let Some(id) = id { load_question(tx, id)?.created_at_ms } else { now };
            let id = id.map(str::to_owned).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let item = IqQuestion { id, content: input.content, reference_answer: input.reference_answer,
                enabled: input.enabled, created_at_ms: created, updated_at_ms: now.max(created) };
            tx.execute("INSERT INTO iq_questions VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET content=excluded.content, reference_answer=excluded.reference_answer,enabled=excluded.enabled,updated_at_ms=excluded.updated_at_ms",
                params![item.id,item.content,item.reference_answer,item.enabled,integer(item.created_at_ms)?,integer(item.updated_at_ms)?])?;
            Ok(item)
        })
    }
    pub(super) fn delete_question(&self, id: &str) -> Result<IqQuestion> {
        self.write(|tx| {
            let item = load_question(tx, id)?;
            tx.execute("DELETE FROM iq_questions WHERE id=?1", [id])?;
            Ok(item)
        })
    }
    pub(super) fn set_question_enabled(&self, id: &str, enabled: bool) -> Result<IqQuestion> {
        self.write(|tx| {
            let mut item = load_question(tx, id)?;
            item.enabled = enabled;
            item.updated_at_ms = now_ms().max(item.created_at_ms);
            tx.execute(
                "UPDATE iq_questions SET enabled=?1,updated_at_ms=?2 WHERE id=?3",
                params![enabled, integer(item.updated_at_ms)?, id],
            )?;
            Ok(item)
        })
    }
    pub(super) fn create_run(
        &self,
        input: IqStartInput,
        participants: Vec<IqParticipant>,
    ) -> Result<IqRun> {
        self.write(|tx| {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM iq_runs WHERE status='running')", [], |r| r.get::<_,bool>(0))? {
                return Err(IqError::AlreadyRunning);
            }
            let questions = input.question_ids.iter().map(|id| load_question(tx, id)).collect::<Result<Vec<_>>>()?;
            if questions.iter().any(|q| !q.enabled) { return Err(invalid("input.questionIds", "停用题目不能参与测试。")); }
            let id = uuid::Uuid::new_v4().to_string();
            let mut entries = Vec::new();
            for question_index in 0..questions.len() {
                for participant_index in 0..participants.len() {
                    entries.push(IqEntry { id: uuid::Uuid::new_v4().to_string(), run_id: id.clone(),
                        question_index, participant_index, status: EntryStatus::Pending, attempt: 0,
                        started_at_ms: None, finished_at_ms: None, first_token_ms: None, total_time_ms: None,
                        input_tokens: None, output_tokens: None, error_code: None, error_message: None, has_raw_stream: false });
                }
            }
            let mut run = IqRun { id, status: RunStatus::Running, questions, participants, entries,
                started_at_ms: now_ms(), finished_at_ms: None, summary: IqRunSummary::default() };
            run.summarize(0);
            tx.execute("INSERT INTO iq_runs VALUES (?1,'running',?2,?3,?4,NULL,0)",
                params![run.id,json(&run.questions)?,json(&run.participants)?,integer(run.started_at_ms)?])?;
            for entry in &run.entries {
                tx.execute("INSERT INTO iq_entries VALUES (?1,?2,?3,?4,'pending',0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,0)",
                    params![entry.id,run.id,i64::try_from(entry.question_index).map_err(|_| IqError::Storage)?,i64::try_from(entry.participant_index).map_err(|_| IqError::Storage)?])?;
            }
            Ok(run)
        })
    }
    pub(super) fn save_run(&self, run: &IqRun) -> Result<()> {
        self.write(|tx| save_run(tx, run))
    }
    pub(super) fn run(&self, id: &str) -> Result<IqRun> {
        self.read(|c| load_run(c, id))
    }
    pub(super) fn runs(&self) -> Result<Vec<IqRun>> {
        self.read(|c| {
            let ids = c
                .prepare("SELECT id FROM iq_runs ORDER BY started_at_ms DESC,id")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.iter().map(|id| load_run(c, id)).collect()
        })
    }
    pub(super) fn contains_run(&self, id: &str) -> Result<bool> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM iq_runs WHERE id=?1)",
                [id],
                |r| r.get(0),
            )?)
        })
    }
    pub(super) fn delete_runs(&self, ids: &[String]) -> Result<()> {
        self.write(|tx| {
            for id in ids {
                let status: Option<String> = tx
                    .query_row("SELECT status FROM iq_runs WHERE id=?1", [id], |r| r.get(0))
                    .optional()?;
                if status.as_deref() == Some("running") {
                    return Err(IqError::AlreadyRunning);
                }
                tx.execute("DELETE FROM iq_runs WHERE id=?1", [id])?;
            }
            Ok(())
        })
    }
    pub(super) fn recover(&self) -> Result<Vec<IqRun>> {
        self.write(|tx| {
            let ids = tx
                .prepare("SELECT id FROM iq_runs WHERE status='running'")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut recovered = Vec::new();
            for id in ids {
                let mut run = load_run(tx, &id)?;
                run.status = RunStatus::Aborted;
                run.finished_at_ms = Some(now_ms().max(run.started_at_ms));
                for entry in &mut run.entries {
                    if !entry.status.terminal() {
                        entry.status = EntryStatus::Aborted;
                        entry.finished_at_ms = run.finished_at_ms;
                        entry.error_code = Some("interrupted".into());
                        entry.error_message =
                            Some("上次运行异常中断，已在启动时收敛；已有证据已保留。".into());
                    }
                }
                // 已知的在线耗时保留，不把离线时间算成真实执行耗时。
                run.summarize(run.summary.duration_ms);
                save_run(tx, &run)?;
                recovered.push(run);
            }
            Ok(recovered)
        })
    }
}
fn question(row: &rusqlite::Row<'_>) -> rusqlite::Result<IqQuestion> {
    Ok(IqQuestion {
        id: row.get(0)?,
        content: row.get(1)?,
        reference_answer: row.get(2)?,
        enabled: row.get(3)?,
        created_at_ms: number(row, 4)?,
        updated_at_ms: number(row, 5)?,
    })
}
fn load_question(c: &Connection, id: &str) -> Result<IqQuestion> {
    c.query_row("SELECT id,content,reference_answer,enabled,created_at_ms,updated_at_ms FROM iq_questions WHERE id=?1", [id], question)
        .optional()?.ok_or(IqError::NotFound)
}
fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|_| IqError::Storage)
}
fn decode<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_str(&text)
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error)))
}
fn state<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    decode(format!("\"{text}\""))
}
fn state_name(value: impl Serialize) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|_| IqError::Storage)?
        .as_str()
        .map(str::to_owned)
        .ok_or(IqError::Storage)
}
fn save_run(tx: &Transaction<'_>, run: &IqRun) -> Result<()> {
    let changed = tx.execute(
        "UPDATE iq_runs SET status=?1,finished_at_ms=?2,duration_ms=?3 WHERE id=?4",
        params![
            state_name(run.status)?,
            optional_integer(run.finished_at_ms)?,
            integer(run.summary.duration_ms)?,
            run.id
        ],
    )?;
    if changed != 1 {
        return Err(IqError::Storage);
    }
    for entry in &run.entries {
        let changed = tx.execute("UPDATE iq_entries SET status=?1,attempt=?2,started_at_ms=?3,finished_at_ms=?4,first_token_ms=?5,total_time_ms=?6,input_tokens=?7,output_tokens=?8,error_code=?9,error_message=?10,has_raw_stream=?11 WHERE id=?12 AND run_id=?13",
            params![state_name(entry.status)?,entry.attempt,optional_integer(entry.started_at_ms)?,optional_integer(entry.finished_at_ms)?,optional_integer(entry.first_token_ms)?,optional_integer(entry.total_time_ms)?,
                optional_integer(entry.input_tokens)?,optional_integer(entry.output_tokens)?,entry.error_code,entry.error_message,entry.has_raw_stream,entry.id,run.id])?;
        if changed != 1 {
            return Err(IqError::Storage);
        }
    }
    Ok(())
}
fn load_run(c: &Connection, id: &str) -> Result<IqRun> {
    let mut run = c.query_row("SELECT id,status,questions_json,participants_json,started_at_ms,finished_at_ms,duration_ms FROM iq_runs WHERE id=?1", [id], |row| {
        let duration_ms = number(row,6)?;
        Ok(IqRun { id: row.get(0)?,status: state(row.get(1)?)?,questions: decode(row.get(2)?)?,participants: decode(row.get(3)?)?,
            started_at_ms: number(row,4)?,finished_at_ms: optional_number(row,5)?,
            entries: Vec::new(),summary: IqRunSummary { duration_ms,..Default::default() } })
    }).optional()?.ok_or(IqError::NotFound)?;
    run.entries = c.prepare("SELECT id,run_id,question_index,participant_index,status,attempt,started_at_ms,finished_at_ms,first_token_ms,total_time_ms,input_tokens,output_tokens,error_code,error_message,has_raw_stream FROM iq_entries WHERE run_id=?1 ORDER BY question_index,participant_index")?
        .query_map([id], |row| Ok(IqEntry { id: row.get(0)?,run_id: row.get(1)?,question_index: index(row,2)?,participant_index: index(row,3)?,
            status: state(row.get(4)?)?,attempt: row.get(5)?,started_at_ms: optional_number(row,6)?,finished_at_ms: optional_number(row,7)?,first_token_ms: optional_number(row,8)?,
            total_time_ms: optional_number(row,9)?,input_tokens: optional_number(row,10)?,output_tokens: optional_number(row,11)?,error_code: row.get(12)?,error_message: row.get(13)?,has_raw_stream: row.get(14)? }))?
        .collect::<rusqlite::Result<_>>()?;
    run.summarize(run.summary.duration_ms);
    Ok(run)
}

fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| IqError::Storage)
}
fn optional_integer(value: Option<u64>) -> Result<Option<i64>> {
    value.map(integer).transpose()
}
fn number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(index, Type::Integer, Box::new(e)))
}
fn optional_number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| {
            u64::try_from(value).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Integer, Box::new(e))
            })
        })
        .transpose()
}
fn index(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<usize> {
    usize::try_from(number(row, column)?)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(e)))
}
