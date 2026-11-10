//! IQ 测试仅使用临时环境、真实 SQLite 和可控的本地 HTTP 服务。
use crate::{
    application::composition::{self, Application},
    modules::{
        configuration,
        iq_test::*,
        monitoring, notifications,
        sites::{self, CreateSite, Protocol, Site, SiteStore},
    },
    shared::{
        diagnostics::Diagnostics,
        model_client::test_support::Response,
        persistence::{test_support::environment, Database},
    },
};
use serde_json::{json, Map, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
pub(crate) const SECRET: &str = "iq-execution-key-stays-in-memory";
pub(crate) struct Fixture {
    pub directory: tempfile::TempDir,
    pub app: Application,
    pub database: Arc<Database>,
    pub sites: SiteStore,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let env = environment(directory.path(), "com.relaypulse.app.dev");
        let database = Arc::new(
            Database::open(
                env.paths(),
                &[
                    configuration::SCHEMA,
                    notifications::SCHEMA,
                    sites::SCHEMA,
                    monitoring::SCHEMA,
                    crate::modules::operation_logs::SCHEMA,
                    crate::modules::iq_test::SCHEMA,
                ],
            )
            .unwrap(),
        );
        let sites = SiteStore::new(database.clone());
        let diagnostics = Arc::new(Diagnostics::new(&env));
        let app = composition::compose(env, diagnostics).unwrap();
        app.iq_test.check_ready().unwrap();
        Self {
            directory,
            app,
            database,
            sites,
        }
    }
    pub(crate) fn root(&self) -> PathBuf {
        self.app.environment.paths().data_root().join("iq-test")
    }
    pub(crate) fn site(
        &self,
        url: &str,
        protocol: Protocol,
        names: &[&str],
        enabled: bool,
    ) -> Site {
        let id = self
            .sites
            .create(
                &CreateSite {
                    name: "原站点".into(),
                    protocol,
                    base_url: url.into(),
                    secret: SECRET.into(),
                    enabled,
                },
                &names.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
            .unwrap();
        self.sites.get(&id).unwrap()
    }
    pub(crate) async fn question(&self, content: &str) -> IqQuestion {
        self.app
            .iq_test
            .save_question(
                IqQuestionInput {
                    content: content.into(),
                    reference_answer: Some("参考答案".into()),
                    enabled: true,
                },
                None,
            )
            .await
            .unwrap()
    }
    pub(crate) async fn participant(
        &self,
        site: &Site,
        index: usize,
        parameters: Value,
    ) -> IqParticipant {
        self.app
            .iq_test
            .create_participant(IqParticipantInput {
                model_key: format!("{}/{}", site.id, site.models[index].id),
                preset: "高 (high)".into(),
                parameters: parameters.as_object().unwrap().clone(),
            })
            .await
            .unwrap()
    }
    pub(crate) async fn available(
        &self,
        site: &Site,
        index: usize,
        parameters: Value,
    ) -> IqParticipant {
        let p = self.participant(site, index, parameters).await;
        let result = self
            .app
            .iq_test
            .precheck(p.snapshot_id.clone())
            .await
            .unwrap();
        assert_eq!(
            result.precheck.status,
            PrecheckStatus::Available,
            "{}",
            result.precheck.message
        );
        self.app
            .iq_test
            .snapshot()
            .await
            .unwrap()
            .participants
            .into_iter()
            .find(|p2| p2.id == p.id)
            .unwrap()
    }
    pub(crate) async fn start(&self, questions: &[IqQuestion]) -> IqRun {
        self.start_with_retries(questions, 0).await
    }
    pub(crate) async fn start_with_retries(
        &self,
        questions: &[IqQuestion],
        retry_max: u32,
    ) -> IqRun {
        self.app
            .iq_test
            .start_run(IqStartInput {
                question_ids: questions.iter().map(|q| q.id.clone()).collect(),
                retry_max,
            })
            .await
            .unwrap()
    }
    pub(crate) async fn terminal(&self, id: &str) -> IqRun {
        terminal(&self.app.iq_test, id).await
    }
    pub(crate) fn count(&self, table: &str) -> i64 {
        assert!([
            "probe_records",
            "probe_states",
            "operation_logs",
            "iq_runs",
            "iq_entries"
        ]
        .contains(&table));
        self.database
            .with_connection(|c| {
                c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            })
            .unwrap()
    }
    pub(crate) fn reopen(&self) -> Application {
        let env = environment(self.directory.path(), "com.relaypulse.app.dev");
        let diagnostics = Arc::new(Diagnostics::new(&env));
        let app = composition::compose(env, diagnostics).unwrap();
        app.iq_test.check_ready().unwrap();
        app
    }
}
pub(crate) async fn terminal(service: &Arc<IqTest>, id: &str) -> IqRun {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let run = service.run_details(id.into()).await.unwrap();
            if run.status != RunStatus::Running {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
pub(crate) fn success(protocol: Protocol) -> Response {
    match protocol {
        Protocol::OpenaiResponses => Response::sse(&[
            ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\n",0),
            ("data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":4,\"output_tokens\":8,\"total_tokens\":12}}}\n\n",0),
        ]),
        Protocol::Anthropic => Response::sse(&[
            ("data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":4}}}\n\n",0),
            ("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"answer\"}}\n\n",0),
            ("data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":8}}\n\n",0),
            ("data: {\"type\":\"message_stop\"}\n\n",0),
        ]),
        _ => Response::sse(&[
            ("data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n",0),
            ("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":8,\"total_tokens\":12}}\n\n",0),
            ("data: [DONE]\n\n",0),
        ]),
    }
}
pub(crate) fn raw(root: &Path, entry: &IqEntry, attempt: u32) -> PathBuf {
    root.join("runs")
        .join(&entry.run_id)
        .join("entries")
        .join(&entry.id)
        .join(format!("response.{attempt}.jsonl"))
}
pub(crate) fn response_body(root: &Path, entry: &IqEntry, attempt: u32) -> Vec<u8> {
    let mut body = Vec::new();
    for line in std::fs::read_to_string(raw(root, entry, attempt))
        .unwrap()
        .lines()
    {
        let event: Value = serde_json::from_str(line).unwrap();
        if event["event"] == "response_chunk" {
            if let Some(text) = event["text"].as_str() {
                body.extend_from_slice(text.as_bytes());
            } else {
                body.extend(serde_json::from_value::<Vec<u8>>(event["bytes"].clone()).unwrap());
            }
        }
    }
    body
}
pub(crate) fn meta(root: &Path, entry: &IqEntry, attempt: u32) -> Value {
    serde_json::from_slice(
        &std::fs::read(raw(root, entry, attempt).with_file_name(format!("meta.{attempt}.json")))
            .unwrap(),
    )
    .unwrap()
}
pub(crate) fn params() -> Map<String, Value> {
    json!({"temperature":0.4,"reasoning_effort":"high"})
        .as_object()
        .unwrap()
        .clone()
}
pub(crate) fn files(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut result = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return result;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}

pub(crate) fn persist_interrupted_run(database: &Database, run: &IqRun) {
    database.with_connection(|c|{
 c.execute("UPDATE iq_runs SET status='running',finished_at_ms=NULL,duration_ms=?2 WHERE id=?1",rusqlite::params![run.id,i64::try_from(run.summary.duration_ms).unwrap()])?;
 for e in &run.entries {c.execute("UPDATE iq_entries SET status=?2,attempt=?3,started_at_ms=?4,finished_at_ms=?5,first_token_ms=?6,total_time_ms=?7,input_tokens=?8,output_tokens=?9,has_raw_stream=?10 WHERE id=?1",rusqlite::params![e.id,serde_json::to_value(e.status).unwrap().as_str().unwrap(),e.attempt,e.started_at_ms.map(|v|v as i64),e.finished_at_ms.map(|v|v as i64),e.first_token_ms.map(|v|v as i64),e.total_time_ms.map(|v|v as i64),e.input_tokens.map(|v|v as i64),e.output_tokens.map(|v|v as i64),e.has_raw_stream])?;}
 Ok(())
 }).unwrap();
}
