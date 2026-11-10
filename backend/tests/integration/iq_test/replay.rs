use crate::{
    iq_test_support::{meta, raw, success, Fixture},
    modules::iq_test::*,
    shared::model_client::{test_support::Server, Protocol},
};
use serde_json::json;

#[tokio::test]
async fn replay_reads_each_saved_attempt_and_keeps_partial_content_without_requesting_a_model() {
    let fixture = Fixture::new();
    let server = Server::new(vec![
        success(Protocol::OpenaiResponses),
        success(Protocol::OpenaiResponses),
    ]);
    let site = fixture.site(&server.url, Protocol::OpenaiResponses, &["model"], true);
    fixture.available(&site, 0, json!({})).await;
    let run = fixture
        .start(&[fixture.question("historical question").await])
        .await;
    let run = fixture.terminal(&run.id).await;
    let entry = &run.entries[0];
    let body = std::fs::read(raw(&fixture.root(), entry, 1)).unwrap();
    let mut first = meta(&fixture.root(), entry, 1);
    let directory = raw(&fixture.root(), entry, 1).parent().unwrap().to_owned();
    let mut second = first.clone();
    second["attempt"] = json!(2);
    std::fs::write(
        directory.join("meta.2.json"),
        serde_json::to_vec(&second).unwrap(),
    )
    .unwrap();
    std::fs::write(directory.join("response.2.jsonl"), &body).unwrap();
    first["status"] = json!("timeout");
    first["errorCode"] = json!("output_timeout");
    first["bodyComplete"] = json!(false);
    std::fs::write(
        directory.join("meta.1.json"),
        serde_json::to_vec(&first).unwrap(),
    )
    .unwrap();
    fixture
        .database
        .with_connection(|c| {
            c.execute("UPDATE iq_entries SET attempt=2 WHERE id=?1", [&entry.id])?;
            Ok(())
        })
        .unwrap();
    let latest = fixture
        .app
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert_eq!(latest.attempt, Some(2));
    assert_eq!(latest.attempts.len(), 2);
    assert!(latest.parts.iter().any(|part| part.text.contains("answer")));
    let earlier = fixture
        .app
        .iq_test
        .entry_output(entry.id.clone(), Some(1))
        .await
        .unwrap();
    assert_eq!(earlier.attempt, Some(1));
    assert_eq!(earlier.attempts[0].status, EntryStatus::Timeout);
    assert!(!earlier.parts.is_empty());
    assert!(fixture
        .app
        .iq_test
        .entry_output(entry.id.clone(), Some(3))
        .await
        .is_err());
    assert!(fixture
        .app
        .iq_test
        .entry_output("../outside".into(), None)
        .await
        .is_err());
    assert_eq!(std::fs::read(raw(&fixture.root(), entry, 1)).unwrap(), body);
    assert_eq!(server.requests().len(), 2);
    assert!(fixture
        .app
        .iq_test
        .snapshot()
        .await
        .unwrap()
        .participants
        .is_empty());
    fixture.app.iq_test.stop().await;
}

#[tokio::test]
async fn existing_sse_history_replays_after_restart_without_rewriting_old_files() {
    let fixture = Fixture::new();
    let server = Server::new(vec![
        success(Protocol::OpenaiResponses),
        success(Protocol::OpenaiResponses),
    ]);
    let site = fixture.site(&server.url, Protocol::OpenaiResponses, &["model"], true);
    fixture.available(&site, 0, json!({})).await;
    let run = fixture
        .start(&[fixture.question("legacy history").await])
        .await;
    let run = fixture.terminal(&run.id).await;
    let entry = &run.entries[0];
    fixture.app.iq_test.stop().await;
    let journal = raw(&fixture.root(), entry, 1);
    let directory = journal.parent().unwrap();
    let body = crate::iq_test_support::response_body(&fixture.root(), entry, 1);
    std::fs::write(directory.join("stream.1.sse"), &body).unwrap();
    std::fs::remove_file(&journal).unwrap();
    let mut details = meta(&fixture.root(), entry, 1);
    details["formatVersion"] = json!(1);
    details.as_object_mut().unwrap().remove("outputFiles");
    std::fs::write(
        directory.join("meta.1.json"),
        serde_json::to_vec(&details).unwrap(),
    )
    .unwrap();
    let before = crate::iq_test_support::files(&fixture.root());
    let reopened = fixture.reopen();
    let output = reopened
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert!(output.parts.iter().any(|part| part.text == "answer"));
    assert!(output.parsing_complete);
    assert_eq!(crate::iq_test_support::files(&fixture.root()), before);
    assert_eq!(server.requests().len(), 2);
    reopened.iq_test.stop().await;
}

#[tokio::test]
async fn interrupted_jsonl_tail_keeps_previous_output_and_reports_partial_parsing() {
    let fixture = Fixture::new();
    let server = Server::new(vec![
        success(Protocol::OpenaiResponses),
        success(Protocol::OpenaiResponses),
    ]);
    let site = fixture.site(&server.url, Protocol::OpenaiResponses, &["model"], true);
    fixture.available(&site, 0, json!({})).await;
    let run = fixture
        .start(&[fixture.question("partial history").await])
        .await;
    let mut run = fixture.terminal(&run.id).await;
    fixture.app.iq_test.stop().await;
    let entry = &mut run.entries[0];
    let path = raw(&fixture.root(), entry, 1);
    let mut journal = std::fs::read(&path).unwrap();
    journal.extend_from_slice(b"{\"event\":\"response_chunk\",\"text\":");
    std::fs::write(&path, &journal).unwrap();
    let mut details = meta(&fixture.root(), entry, 1);
    details["status"] = json!("running");
    std::fs::write(
        path.with_file_name("meta.1.json"),
        serde_json::to_vec(&details).unwrap(),
    )
    .unwrap();
    entry.status = EntryStatus::Running;
    run.status = RunStatus::Running;
    crate::iq_test_support::persist_interrupted_run(&fixture.database, &run);
    let reopened = fixture.reopen();
    let entry = &run.entries[0];
    let output = reopened
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert_eq!(output.attempts[0].status, EntryStatus::Aborted);
    assert!(output.parts.iter().any(|part| part.text == "answer"));
    assert!(!output.parsing_complete);
    assert_eq!(std::fs::read(&path).unwrap(), journal);
    assert_eq!(server.requests().len(), 2);
    reopened.iq_test.stop().await;
}
