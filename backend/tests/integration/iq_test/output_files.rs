use super::*;
use std::fs;

#[tokio::test]
async fn html_answer_is_saved_as_html_and_process_jsonl_replays_without_mixing_reasoning() {
    let f = Fixture::new();
    let html = "<!doctype html>\n<html><body>鹈鹕骑自行车</body></html>\n";
    let answer = format!("下面是页面：\n```html\n{html}```\n");
    let thinking = format!(
        "data: {}\n\n",
        json!({"choices":[{"delta":{"reasoning_content":"先分析题目"}}]})
    );
    let text = format!(
        "data: {}\n\n",
        json!({"choices":[{"delta":{"content":answer}}]})
    );
    let response = Response::sse(&[(&thinking, 0), (&text, 0), ("data: [DONE]\n\n", 0)]);
    let expected: Vec<u8> = response
        .parts
        .iter()
        .flat_map(|(_, bytes)| bytes.clone())
        .collect();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible), response]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f.start(&[f.question("generate html").await]).await;
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Completed);
    let file = raw(&f.root(), entry, 1);
    let directory = file.parent().unwrap();
    let mut names: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|file| file.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["meta.1.json", "output.1.html", "response.1.jsonl"]);
    assert_eq!(
        fs::read_to_string(directory.join("output.1.html")).unwrap(),
        html
    );
    let metadata = meta(&f.root(), entry, 1);
    assert_eq!(metadata["formatVersion"], 2);
    assert_eq!(metadata["outputFiles"], json!(["output.1.html"]));
    assert_eq!(metadata["streamBytes"], expected.len());
    assert_eq!(support::response_body(&f.root(), entry, 1), expected);
    let events: Vec<Value> = fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.first().unwrap()["event"], "response_started");
    assert_eq!(events.last().unwrap()["event"], "request_finished");
    let replay = f
        .app
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert!(replay.parsing_complete);
    assert!(replay.parts.iter().any(|part| part.kind
        == crate::shared::model_client::OutputKind::Text
        && part.text == answer));
    assert!(replay.parts.iter().any(|part| part.kind
        == crate::shared::model_client::OutputKind::Reasoning
        && part.text == "先分析题目"));
    assert_eq!(server.requests().len(), 2);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn output_files_follow_svg_multiple_code_blocks_and_markdown_content() {
    for (answer, expected) in [
        (
            "<svg viewBox=\"0 0 1 1\"><circle r=\"1\"/></svg>",
            vec![(
                "output.1.svg",
                "<svg viewBox=\"0 0 1 1\"><circle r=\"1\"/></svg>",
            )],
        ),
        (
            "```html\n<div>页面</div>\n```\n```css\ndiv { color: red; }\n```",
            vec![
                ("output.1.1.html", "<div>页面</div>\n"),
                ("output.1.2.css", "div { color: red; }\n"),
            ],
        ),
        (
            "## 答案\n最少需要 18 颗糖果。",
            vec![("output.1.md", "## 答案\n最少需要 18 颗糖果。")],
        ),
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![
            success(Protocol::OpenaiCompatible),
            Response::json(
                json!({"choices":[{"message":{"content":answer},"finish_reason":"stop"}]}),
            ),
        ]);
        let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
        f.available(&site, 0, json!({})).await;
        let run = f.start(&[f.question("save answer files").await]).await;
        let run = f.terminal(&run.id).await;
        let entry = &run.entries[0];
        assert_eq!(entry.status, EntryStatus::Completed);
        let directory = raw(&f.root(), entry, 1).parent().unwrap().to_owned();
        assert_eq!(
            meta(&f.root(), entry, 1)["outputFiles"],
            json!(expected.iter().map(|(name, _)| name).collect::<Vec<_>>())
        );
        for (name, content) in expected {
            assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), content);
        }
        assert!(!directory.join("output.1.txt").exists());
        f.app.iq_test.stop().await;
    }
}

#[tokio::test]
async fn jsonl_preserves_split_utf8_and_non_utf8_error_body_bytes() {
    let f = Fixture::new();
    let mut response = Response::raw(400, "text/plain", "");
    let mut expected = vec![b'x'; 16 * 1024 - 1];
    expected.extend_from_slice("糖果".as_bytes());
    expected.extend_from_slice(&[0xff, 0, b'\n']);
    response.parts = vec![(Duration::ZERO, expected.clone())];
    let server = Server::new(vec![success(Protocol::OpenaiCompatible), response]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f.start(&[f.question("retain error body").await]).await;
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Error);
    assert_eq!(support::response_body(&f.root(), entry, 1), expected);
    assert_eq!(meta(&f.root(), entry, 1)["streamBytes"], expected.len());
    assert_eq!(meta(&f.root(), entry, 1)["outputFiles"], json!([]));
    let replay = f
        .app
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert!(replay.raw_response.is_some());
    f.app.iq_test.stop().await;
}
