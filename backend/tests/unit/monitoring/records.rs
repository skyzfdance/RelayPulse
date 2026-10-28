use super::*;
use crate::{
    modules::{
        configuration, monitoring,
        sites::{self, CreateSite, Protocol, SiteStore},
    },
    shared::persistence::{test_support::environment, Database},
};
use rusqlite::{named_params, params};
use serde_json::{json, Value};
use std::sync::Arc;
const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 86_400_000;
struct Fixture {
    _temp: tempfile::TempDir,
    database: Arc<Database>,
    records: RecordStore,
    sites: SiteStore,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let env = environment(temp.path(), "com.relaypulse.app.dev");
        let database = Arc::new(
            Database::open(
                env.paths(),
                &[configuration::SCHEMA, sites::SCHEMA, monitoring::SCHEMA],
            )
            .unwrap(),
        );
        Self {
            records: RecordStore::new(database.clone()),
            sites: SiteStore::new(database.clone()),
            database,
            _temp: temp,
        }
    }
    fn site(&self) -> (String, String) {
        let id = self
            .sites
            .create(
                &CreateSite {
                    name: "current name".into(),
                    protocol: Protocol::OpenaiResponses,
                    base_url: "http://127.0.0.1:1".into(),
                    secret: "test-secret".into(),
                    enabled: false,
                },
                &["current model".into()],
            )
            .unwrap();
        let model = self.sites.get(&id).unwrap().models[0].id.clone();
        (id, model)
    }
    fn seed(
        &self,
        site: &(String, String),
        id: &str,
        time: i64,
        name: &str,
        outcome: &str,
        usage: [Option<i64>; 3],
    ) {
        self.database.with_connection(|c| c.execute("INSERT INTO probe_records
            (id,site_id,model_id,run_kind,success,status_code,duration_ms,finished_at_ms,started_at_ms,
            site_name,model_name,result_kind,input_tokens,output_tokens,total_tokens,capture_enabled)
            VALUES (:id,:site,:model,'manual',:success,200,0,:time,:time,:name,'historical model',:outcome,:input,:output,:total,0)",
            named_params! {":id":id,":site":site.0,":model":site.1,":success":outcome!="failed",":time":time,
                ":name":name,":outcome":outcome,":input":usage[0],":output":usage[1],":total":usage[2]})).unwrap();
    }
}
fn query() -> ProbeRecordsQuery {
    ProbeRecordsQuery {
        range: "all".into(),
        site_id: None,
        model_name: None,
        result: None,
        page: 1,
        page_size: 10,
    }
}
fn filter(query: ProbeRecordsQuery) -> query::RecordFilter {
    query.validate().unwrap();
    query.at(NOW as u64, 30).unwrap()
}
fn value(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}
#[test]
fn filters_stable_order_retention_nulls_zero_and_site_aggregation_use_historical_facts() {
    let f = Fixture::new();
    let a = f.site();
    let b = f.site();
    f.seed(
        &a,
        "a",
        NOW - 100,
        "latest snapshot",
        "succeeded",
        [Some(0), None, Some(0)],
    );
    f.seed(
        &a,
        "b",
        NOW - 100,
        "other tied name",
        "failed",
        [None, Some(10), Some(10)],
    );
    f.seed(
        &a,
        "c",
        NOW - 2 * DAY,
        "old name",
        "succeeded",
        [Some(3), Some(5), None],
    );
    f.seed(
        &b,
        "d",
        NOW - 200,
        "latest snapshot",
        "slow",
        [Some(0), None, None],
    );
    f.seed(
        &a,
        "expired",
        NOW - 31 * DAY,
        "expired",
        "succeeded",
        [None, None, Some(999)],
    );
    f.seed(
        &a,
        "future",
        NOW + 1,
        "future",
        "succeeded",
        [None, None, Some(999)],
    );
    let list = f.records.list(filter(query())).unwrap();
    assert_eq!(
        list.items.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "d", "c"]
    );
    assert_eq!((list.total, list.retained_count, list.page), (4, 4, 1));
    let stats = value(f.records.token_stats(filter(query())).unwrap());
    assert_eq!(stats["total"], 2);
    assert_eq!(stats["retainedCount"], 4);
    assert_eq!(
        stats["summary"],
        json!({"probeCount":4,"reportedCount":4,"totalReportedCount":2,"inputTokens":3,"outputTokens":15,"totalTokens":10,"averageTokens":5.0})
    );
    assert_eq!(
        stats["items"][0],
        json!({"siteId":a.0,"siteName":"latest snapshot","probeCount":3,"reportedCount":3,
        "totalReportedCount":2,"inputTokens":3,"outputTokens":15,"totalTokens":10,"averageTokens":5.0})
    );
    assert_eq!(stats["items"][1]["siteId"], b.0);
    assert_eq!(stats["items"][1]["siteName"], "latest snapshot");
    assert_eq!(stats["items"][1]["inputTokens"], 0);
    assert!(stats["items"][1]["totalTokens"].is_null());
    let mut q = query();
    q.range = "24h".into();
    q.result = Some("failure".into());
    let list = f.records.list(filter(q)).unwrap();
    assert_eq!((list.total, list.retained_count), (1, 4));
    assert_eq!(list.items[0].id, "b");
    let mut q = query();
    q.site_id = Some(a.0.clone());
    q.model_name = Some("historical model".into());
    assert_eq!(f.records.list(filter(q.clone())).unwrap().total, 3);
    q.model_name = Some("current model".into());
    let list = f.records.list(filter(q)).unwrap();
    assert_eq!((list.total, list.retained_count, list.page), (0, 4, 1));
    assert!(f
        .records
        .get("expired", (NOW - 30 * DAY) as u64, NOW as u64)
        .unwrap()
        .is_none());
    assert!(f
        .records
        .get("future", (NOW - 30 * DAY) as u64, NOW as u64)
        .unwrap()
        .is_none());
}
#[test]
fn request_start_controls_time_sorting_range_includes_start_and_retention_uses_finish() {
    let f = Fixture::new();
    let site = f.site();
    for (id, time) in [
        ("boundary", NOW - DAY),
        ("before", NOW - DAY - 1),
        ("now", NOW),
        ("retention", NOW - 30 * DAY),
    ] {
        f.seed(&site, id, time, id, "succeeded", [None, None, None]);
    }
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET finished_at_ms=?1 WHERE id='before'",
                [NOW - 1],
            )?;
            c.execute(
                "UPDATE probe_records SET finished_at_ms=?1 WHERE id='boundary'",
                [NOW - 2],
            )?;
            Ok(())
        })
        .unwrap();
    let mut q = query();
    q.range = "24h".into();
    let rows = f.records.list(filter(q)).unwrap();
    assert_eq!(
        rows.items.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["now", "boundary"]
    );
    assert_eq!(rows.retained_count, 4);
    let mut q = query();
    q.range = "30d".into();
    assert_eq!(f.records.list(filter(q)).unwrap().total, 4);
}
#[test]
fn both_pagers_clamp_after_filtering_and_summary_is_independent_of_page() {
    let f = Fixture::new();
    for index in 0..12 {
        let site = f.site();
        f.seed(
            &site,
            &format!("record-{index:02}"),
            NOW - 1,
            "same name",
            "succeeded",
            [None, None, Some(index)],
        );
    }
    let mut q = query();
    q.page = 999;
    let list = f.records.list(filter(q.clone())).unwrap();
    assert_eq!((list.page, list.total, list.retained_count), (2, 12, 12));
    assert_eq!(
        list.items.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["record-10", "record-11"]
    );
    let stats = value(f.records.token_stats(filter(q)).unwrap());
    assert_eq!(stats["page"], 2);
    assert_eq!(stats["items"].as_array().unwrap().len(), 2);
    assert_eq!(stats["items"][0]["totalTokens"], 1);
    assert_eq!(stats["items"][1]["totalTokens"], 0);
    assert_eq!(stats["summary"]["totalTokens"], 66);
    assert_eq!(stats["summary"]["totalReportedCount"], 12);
    assert_eq!(stats["summary"]["averageTokens"], 5.5);
    let mut q = query();
    q.site_id = Some("missing".into());
    q.page = 999;
    let stats = value(f.records.token_stats(filter(q)).unwrap());
    assert_eq!(stats["page"], 1);
    assert_eq!(stats["retainedCount"], 12);
    assert_eq!(stats["items"], json!([]));
    assert_eq!(
        stats["summary"],
        json!({"probeCount":0,"reportedCount":0,"totalReportedCount":0,"inputTokens":null,"outputTokens":null,"totalTokens":null,"averageTokens":null})
    );
}
#[test]
fn token_ties_and_unreported_sites_use_id_order_and_do_not_estimate_missing_total() {
    let f = Fixture::new();
    let mut ids = vec![];
    for index in 0..4 {
        let site = f.site();
        ids.push(site.0.clone());
        f.seed(
            &site,
            &format!("r-{index}"),
            NOW - 1,
            "same",
            "succeeded",
            if index < 2 {
                [None, None, Some(0)]
            } else {
                [Some(1), Some(2), None]
            },
        );
    }
    let stats = f.records.token_stats(filter(query())).unwrap();
    let mut reported = ids[..2].to_vec();
    reported.sort();
    let mut missing = ids[2..].to_vec();
    missing.sort();
    reported.extend(missing);
    assert_eq!(
        stats
            .records
            .items
            .iter()
            .map(|r| r.site_id.clone())
            .collect::<Vec<_>>(),
        reported
    );
    assert_eq!(stats.summary.total_tokens, Some(0));
    assert_eq!(stats.summary.average_tokens, Some(0.0));
}
#[test]
fn missing_snapshots_and_historical_thresholds_are_projected_without_current_configuration() {
    let f = Fixture::new();
    let site = f.site();
    f.seed(
        &site,
        "old",
        NOW - 1,
        "old",
        "succeeded",
        [None, None, None],
    );
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET site_name=NULL,model_name=NULL,
        started_at_ms=NULL,result_kind=NULL,status_code=NULL WHERE id='old'",
                [],
            )
        })
        .unwrap();
    let read = || {
        f.records
            .get("old", (NOW - 30 * DAY) as u64, NOW as u64)
            .unwrap()
            .unwrap()
    };
    let detail = value(read().detail(None).unwrap());
    assert_eq!(detail["siteName"], "未知站点");
    assert_eq!(detail["modelName"], "未知模型");
    assert_eq!(detail["slowThresholds"], Value::Null);
    assert_eq!(detail["slowEvaluation"], Value::Null);
    assert_eq!(detail["usage"], Value::Null);
    assert_eq!(detail["usageSource"], "unreported");
    assert_eq!(detail["responseSummary"]["state"], "unknown");
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET duration_ms=2500,
        slow_first_token_ms=1000,slow_total_ms=2000,input_tokens=0 WHERE id='old'",
                [],
            )
        })
        .unwrap();
    let detail = value(read().detail(None).unwrap());
    assert_eq!(detail["result"], "slow");
    assert_eq!(
        detail["slowEvaluation"],
        json!({"status":"evaluated","firstToken":{"comparison":null,"triggered":false},"duration":{"comparison":"gte","triggered":true}})
    );
    assert_eq!(detail["usage"]["inputTokens"], 0);
    assert_eq!(detail["usage"]["totalTokens"], Value::Null);
    f.database.with_connection(|c| c.execute("UPDATE probe_records SET success=0,result_kind='failed',first_token_ms=2000 WHERE id='old'",[])).unwrap();
    let detail = value(read().detail(None).unwrap());
    assert_eq!(detail["result"], "failure");
    assert_eq!(
        detail["slowEvaluation"],
        json!({"status":"skipped_failure","firstToken":{"comparison":null,"triggered":false},"duration":{"comparison":null,"triggered":false}})
    );
    f.database.with_connection(|c| c.execute("UPDATE probe_records SET success=1,result_kind='succeeded',duration_ms=2000,policy_json=?1,protocol='openai-responses' WHERE id='old'",
        params![json!({"source":"model_policy","parameters":[{"key":"reasoning.effort","type":"string","value":"low"},{"key":"reasoning","type":"json","value":"{\"budget\":10}"}]}).to_string()])).unwrap();
    let detail = value(read().detail(None).unwrap());
    assert_eq!(detail["result"], "success");
    assert_eq!(detail["slowEvaluation"]["duration"]["comparison"], "gte");
    assert_eq!(detail["slowEvaluation"]["duration"]["triggered"], false);
    assert_eq!(detail["requestInfo"]["strategyLabel"], "模型独立策略");
}
