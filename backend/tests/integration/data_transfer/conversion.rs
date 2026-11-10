use super::*;
fn legacy(root: &Path, count: usize) -> Connection {
    legacy_with_sql(root, count, str::to_owned)
}
fn legacy_with_sql(root: &Path, count: usize, rewrite: impl Fn(&str) -> String) -> Connection {
    fs::create_dir(root).unwrap();
    let c = Connection::open(root.join("relaypulse.sqlite3")).unwrap();
    c.execute_batch(LEGACY_HISTORY).unwrap();
    for (i, (_, sql)) in LEGACY.iter().take(count).enumerate() {
        c.execute_batch(&rewrite(sql)).unwrap();
        c.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2)",
            rusqlite::params![i as i64, legacy_names()[i]],
        )
        .unwrap();
    }
    c
}
#[tokio::test]
async fn copies_every_value_relationship_template_and_evidence_without_touching_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let target = temp.path().join("new");
    let c = legacy(&source, 7);
    c.execute_batch("UPDATE general_settings SET history_retention_days=77,theme='dark'; UPDATE notification_settings SET enabled=1; UPDATE notification_rules SET status=0,custom_template='{\"templateType\":\"custom\",\"markerId\":\"none\",\"titleTemplate\":\"historical\",\"bodyTemplate\":\"retained\"}' WHERE id='iq.run_completed';").unwrap();
    c.execute(
        "INSERT INTO iq_questions VALUES ('question','题面','参考答案',1,1,1)",
        [],
    )
    .unwrap();
    let question = serde_json::json!({"id":"question","content":"题面","referenceAnswer":"参考答案","enabled":true,"createdAtMs":1,"updatedAtMs":1});
    let participant = serde_json::json!({"id":"participant","snapshotId":"snapshot","modelKey":"site/model","modelName":"historical-model","siteId":"site","siteName":"historical-site","protocol":"openai-compatible","health":"healthy","endpoint":"https://example.test/v1/chat/completions","createdAtMs":1,"preset":"custom","parameters":{"temperature":0.3},"precheck":{"status":"available","message":"ok","checkedAtMs":1}});
    c.execute(
        "INSERT INTO iq_runs VALUES ('run','completed',?1,?2,120,5,1,2,1)",
        rusqlite::params![
            serde_json::json!([question]).to_string(),
            serde_json::json!([participant]).to_string()
        ],
    )
    .unwrap();
    let connection = serde_json::to_string(&crate::modules::sites::Connection::for_protocol(
        crate::modules::sites::Protocol::OpenaiCompatible,
    ))
    .unwrap();
    let policy = serde_json::to_string(&crate::modules::sites::ProbePolicy::default()).unwrap();
    c.execute("INSERT INTO sites VALUES ('site','historical-site','openai-compatible','https://example.test',1,1,?1,?2,0)",rusqlite::params![connection,policy]).unwrap();
    c.execute_batch("INSERT INTO site_credentials VALUES ('site','fixture-secret-preserved'); INSERT INTO site_models VALUES ('model','site','historical-model',0,1,1,NULL,NULL); INSERT INTO probe_states VALUES ('model','healthy',2,9,0,1,2);").unwrap();
    c.execute_batch("INSERT INTO probe_records(id,site_id,model_id,run_kind,success,status_code,first_token_ms,duration_ms,finished_at_ms,site_name,model_name,protocol,started_at_ms,request_method,request_endpoint,result_kind,input_tokens,output_tokens,total_tokens,response_kind,received_bytes,body_complete,capture_enabled) VALUES ('probe','site','model','manual',1,200,2,9,2,'historical-site','historical-model','openai-compatible',1,'POST','https://example.test/v1/chat/completions','succeeded',3,4,7,'event_stream',6,1,1);").unwrap();
    c.execute_batch(r#"INSERT INTO operation_logs VALUES ('operation',1,'1970-01-01T00:00:00.001Z','user','本机用户','historical.unknown','historical-group','原动作名称','historical-site','success','原摘要','{"kind":"drawer"}','原说明','{"kind":"snapshot","fields":[]}');"#).unwrap();
    let probe_dir = source.join("probe-records/probe");
    fs::create_dir_all(&probe_dir).unwrap();
    let events = [
        serde_json::json!({"event":"record_created","recordId":"probe","formatVersion":2}),
        serde_json::json!({"event":"request_started","method":"POST","body":{"model":"historical-model","temperature":0.3}}),
        serde_json::json!({"event":"response_started","kind":"event_stream","statusCode":200}),
        serde_json::json!({"event":"sse","data":"answer","terminated":true}),
        serde_json::json!({"event":"request_finished","bodyComplete":true,"receivedTotal":6}),
    ];
    fs::write(
        probe_dir.join("response.jsonl"),
        events
            .iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>(),
    )
    .unwrap();
    c.execute("INSERT INTO iq_entries(id,run_id,question_index,participant_index,status,attempt,has_raw_stream) VALUES ('entry','run',0,0,'completed',2,1)",[]).unwrap();
    let dir = source.join("iq-test/runs/run/entries/entry");
    fs::create_dir_all(&dir).unwrap();
    let raw = b"data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\ndata: [DONE]\n\n";
    for n in 1..=2 {
        fs::write(dir.join(format!("stream.{n}.sse")), raw).unwrap();
        fs::write(dir.join(format!("output.{n}.txt")), "answer").unwrap();
        fs::write(dir.join(format!("meta.{n}.json")),serde_json::to_vec(&serde_json::json!({"formatVersion":1,"runId":"run","entryId":"entry","attempt":n,"bodyComplete":true,"streamBytes":raw.len()})).unwrap()).unwrap();
    }
    drop(c);
    let before = fs::read(source.join("relaypulse.sqlite3")).unwrap();
    let report = convert_offline(&source, &target).unwrap();
    assert_eq!(report.iq_attempts, 2);
    assert_eq!(report.probe_packages, 1);
    assert_eq!(report.tables["iq_questions"], 1);
    assert_eq!(report.tables["iq_entries"], 1);
    assert!(!target.join("conversion-incomplete").exists());
    assert_eq!(fs::read(source.join("relaypulse.sqlite3")).unwrap(), before);
    let db = std::sync::Arc::new(
        Database::open_directory(&target.canonicalize().unwrap(), BASELINE).unwrap(),
    );
    let replay =
        crate::modules::iq_test::verify_history(db.clone(), &target.join("iq-test")).unwrap();
    assert_eq!(
        replay[0].participants[0].candidate.model_name,
        "historical-model"
    );
    assert_eq!(replay[0].participants[0].parameters["temperature"], 0.3);
    assert_eq!(replay[0].questions[0].content, "题面");
    assert_eq!(replay[0].entries[0].attempt, 2);
    let logs = std::sync::Arc::new(relaypulse_operations::OperationLogs::new(db.clone()));
    let detail = logs.detail("operation".into()).await.unwrap();
    assert_eq!(detail.item.action, "historical.unknown");
    assert_eq!(detail.item.action_label, "原动作名称");
    assert_eq!(detail.item.action_group, "historical-group");
    db.with_connection(|c| {
        assert_eq!(
            c.query_row(
                "SELECT secret FROM site_credentials WHERE site_id='site'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "fixture-secret-preserved"
        );
        assert_eq!(
            c.query_row(
                "SELECT total_tokens FROM probe_records WHERE id='probe'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            7
        );
        assert_eq!(
            c.query_row(
                "SELECT history_retention_days FROM general_settings",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            77
        );
        assert_eq!(
            c.query_row(
                "SELECT status FROM notification_rules WHERE id='iq.run_completed'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
    for n in 1..=2 {
        assert_eq!(
            fs::read(target.join(format!("iq-test/runs/run/entries/entry/stream.{n}.sse")))
                .unwrap(),
            raw
        );
    }
    assert!(convert_offline(&source, &target).is_err());
}
#[test]
fn supports_pre_iq_data_and_refuses_unknown_sql_or_evidence_without_repair() {
    for count in [5, 6, 7] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy(&source, count));
        convert_offline(&source, &temp.path().join("new")).unwrap();
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let c = legacy(&source, 7);
    c.execute_batch("CREATE TABLE unexpected(secret TEXT)")
        .unwrap();
    drop(c);
    let before = fs::read(source.join("relaypulse.sqlite3")).unwrap();
    assert!(convert_offline(&source, &temp.path().join("new")).is_err());
    assert_eq!(fs::read(source.join("relaypulse.sqlite3")).unwrap(), before);
}
#[test]
fn unknown_evidence_version_never_publishes_a_ready_directory() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    drop(legacy(&source, 7));
    let dir = source.join("probe-records/record");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("response.jsonl"),
        "{\"event\":\"record_created\",\"formatVersion\":999}\n",
    )
    .unwrap();
    let destination = temp.path().join("new");
    assert!(convert_offline(&source, &destination).is_err());
    assert!(destination.join("conversion-incomplete").exists());
    assert!(source.join("relaypulse.sqlite3").exists());
}

#[test]
fn wal_snapshot_and_unrecognized_adjacent_files_are_preserved_without_opening_source_sqlite() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let c = legacy(&source, 7);
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; UPDATE general_settings SET history_retention_days=88;").unwrap();
    fs::write(
        source.join("relaypulse.sqlite3.notes"),
        b"must retain adjacent evidence",
    )
    .unwrap();
    let before = database_fingerprints(&source).unwrap();
    let target = temp.path().join("new");
    let report = convert_offline(&source, &target).unwrap();
    assert_eq!(report.source_database_files, before);
    assert_eq!(database_fingerprints(&source).unwrap(), before);
    assert_eq!(
        fs::read(target.join("relaypulse.sqlite3.notes")).unwrap(),
        b"must retain adjacent evidence"
    );
    let db = Database::open_directory(&target.canonicalize().unwrap(), BASELINE).unwrap();
    db.with_connection(|c| {
        assert_eq!(
            c.query_row(
                "SELECT history_retention_days FROM general_settings",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            88
        );
        Ok(())
    })
    .unwrap();
    drop(c);
}
#[test]
fn reserved_names_and_links_are_never_overwritten_or_followed() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    drop(legacy(&source, 7));
    fs::write(source.join("original.sqlite3"), b"existing user file").unwrap();
    assert!(convert_offline(&source, &temp.path().join("new")).is_err());
    assert!(!temp.path().join("new").exists());
    assert_eq!(
        fs::read(source.join("original.sqlite3")).unwrap(),
        b"existing user file"
    );
}

#[test]
fn configuration_that_cannot_be_loaded_never_publishes_a_destination() {
    for invalid in [
        "INSERT INTO sites VALUES ('site','name','openai-compatible','https://example.test',1,1,'{}','{}',0)",
        r#"INSERT INTO operation_logs VALUES ('bad',1,'1970-01-01T00:00:00Z','user','user','old-action','old-group','old-label','object','success','summary','{"kind":"drawer"}','explanation','{"kind":"unrecognized"}')"#,
    ] {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let c = legacy(&source, 7);
    c.execute(invalid,[]).unwrap();
    drop(c);
    let before = database_fingerprints(&source).unwrap();
    let destination = temp.path().join("new");
    assert!(convert_offline(&source, &destination).is_err());
    assert_eq!(database_fingerprints(&source).unwrap(), before);
    assert!(destination.join("conversion-incomplete").exists());
    assert!(Database::open_directory(&destination.canonicalize().unwrap(), BASELINE).is_err());
    }
}

#[test]
fn supported_probe_history_uses_one_contract_without_rewriting_source() {
    for header in [
        "{\"event\":\"record_created\",\"recordId\":\"record\",\"formatVersion\":2}\n",
        "{\"event\":\"record_created\",\"recordId\":\"record\"}\n",
        "",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy(&source, 7));
        let dir = source.join("probe-records/record");
        fs::create_dir_all(&dir).unwrap();
        let bytes = format!("{header}{{\"event\":\"request_started\",\"method\":\"POST\"}}\n{{\"event\":\"response_started\",\"kind\":\"json\"}}\n{{\"event\":\"json\",\"value\":{{\"answer\":\"OK\"}}}}\n{{\"event\":\"request_finished\",\"bodyComplete\":true}}\n");
        fs::write(dir.join("response.jsonl"), &bytes).unwrap();
        let root = source.join("probe-records");
        assert_eq!(
            crate::modules::monitoring::verify_capture_history(&root).unwrap(),
            1
        );
        crate::modules::monitoring::inspect_capture_formats(&root).unwrap();
        let target = temp.path().join("new");
        assert_eq!(convert_offline(&source, &target).unwrap().probe_packages, 1);
        assert_eq!(
            crate::modules::monitoring::verify_capture_history(&target.join("probe-records"))
                .unwrap(),
            1
        );
        assert_eq!(
            fs::read(dir.join("response.jsonl")).unwrap(),
            bytes.as_bytes()
        );
        assert_eq!(
            fs::read(target.join("probe-records/record/response.jsonl")).unwrap(),
            bytes.as_bytes()
        );
    }
}

#[test]
fn damaged_or_future_probe_history_is_rejected_consistently_without_source_changes() {
    for bytes in [
        "{\"event\":\"record_created\",\"formatVersion\":999}\n",
        "{broken\n",
        "{\"event\":123}\n",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy(&source, 7));
        let dir = source.join("probe-records/record");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("response.jsonl"), bytes).unwrap();
        let before = database_fingerprints(&source).unwrap();
        let root = source.join("probe-records");
        assert!(crate::modules::monitoring::verify_capture_history(&root).is_err());
        assert!(crate::modules::monitoring::inspect_capture_formats(&root).is_err());
        assert!(convert_offline(&source, &temp.path().join("new")).is_err());
        assert_eq!(database_fingerprints(&source).unwrap(), before);
        assert_eq!(
            fs::read(dir.join("response.jsonl")).unwrap(),
            bytes.as_bytes()
        );
    }
}

#[tokio::test]
async fn legacy_operation_payloads_survive_conversion_and_business_decoding() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let c = legacy(&source, 7);
    let behavior =
        r#"{"kind":"probe_records","recordIds":["historical"],"availability":"available"}"#;
    let content = r#"{"kind":"probe_failure","reason":"historical reason"}"#;
    c.execute("INSERT INTO operation_logs VALUES ('legacy-operation',1,'1970-01-01T00:00:00Z','user','actor','manual_probe','probe','label','object','failure','summary',?1,'explanation',?2)", rusqlite::params![behavior,content]).unwrap();
    drop(c);
    let target = temp.path().join("new");
    convert_offline(&source, &target).unwrap();
    let db = std::sync::Arc::new(
        Database::open_directory(&target.canonicalize().unwrap(), BASELINE).unwrap(),
    );
    let logs = std::sync::Arc::new(
        relaypulse_operations::OperationLogs::new(db.clone()).with_history_decoder(
            std::sync::Arc::new(crate::modules::monitoring::operation_history::ProbeHistoryDecoder),
        ),
    );
    let detail = logs.detail("legacy-operation".into()).await.unwrap();
    assert!(matches!(
        detail.content,
        relaypulse_operations::OperationLogContent::Presentation { .. }
    ));
    assert!(matches!(
        detail.item.detail_behavior,
        relaypulse_operations::DetailBehavior::Navigate { .. }
    ));
    let saved: (String, String) = db
        .with_connection(|c| {
            c.query_row(
                "SELECT detail_behavior,content FROM operation_logs WHERE id='legacy-operation'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(saved, (behavior.into(), content.into()));
}

#[test]
fn november_5_comment_variants_convert_without_changing_source_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    let c = legacy_with_sql(&source, 5, |sql| {
        sql.replace(
            "  -- 同时作为自动探测总开关，主窗口打开时也生效；关闭仅停止新派发，不中断已执行的请求。\n",
            "",
        )
        .replace(
            "  -- 小面板展示及自动探测参与开关：0 = 不展示且不自动探测，1 = 展示并允许自动探测。\n  -- 自动探测还要求通用设置 continue_in_background 和对应模型 enabled 均开启；不影响同步或手动探测。",
            "  -- 小面板展示开关：0 = 不展示，1 = 展示；不影响模型目录同步、手动或周期探测。",
        )
        .replace("首段非空思考或回答文本耗时", "首段有效文本耗时")
    });
    c.execute_batch("UPDATE general_settings SET history_retention_days=71,theme='dark';")
        .unwrap();
    let control = legacy(&temp.path().join("frozen"), 5);
    let actual = schema_objects(&c).unwrap();
    let frozen = schema_objects(&control).unwrap();
    assert_eq!(
        actual
            .iter()
            .zip(&frozen)
            .filter_map(|((name, sql), (_, expected))| (sql != expected).then_some(name.as_str()))
            .collect::<Vec<_>>(),
        ["general_settings", "probe_records", "probe_states", "sites"]
    );
    drop(c);
    fs::write(
        source.join("retained-note.txt"),
        b"-- original /* unchanged */  bytes",
    )
    .unwrap();
    let before = database_fingerprints(&source).unwrap();
    let destination = temp.path().join("new");
    let report = convert_offline(&source, &destination).unwrap();
    assert_eq!(report.source_database_files, before);
    assert_eq!(database_fingerprints(&source).unwrap(), before);
    assert_eq!(
        fs::read(source.join("retained-note.txt")).unwrap(),
        b"-- original /* unchanged */  bytes"
    );
    assert_eq!(
        fs::read(destination.join("retained-note.txt")).unwrap(),
        b"-- original /* unchanged */  bytes"
    );
    assert!(destination.join("conversion-verification.json").is_file());
    assert!(!destination.join("conversion-incomplete").exists());
    let original = Connection::open_with_flags(
        destination.join("original.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(schema_objects(&original).unwrap(), actual);
    let converted =
        Database::open_directory(&destination.canonicalize().unwrap(), BASELINE).unwrap();
    converted
        .with_connection(|db| {
            assert_eq!(
                db.query_row(
                    "SELECT history_retention_days,theme FROM general_settings",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                )?,
                (71, "dark".into())
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn legacy_layout_changes_convert_for_each_existing_history_prefix() {
    for count in [5, 6, 7] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy_with_sql(&source, count, |sql| {
            sql.lines()
                .map(|line| {
                    if line.trim_start().starts_with("--") {
                        line.to_owned()
                    } else {
                        line.replace(" INTEGER ", "\tINTEGER\n")
                            .replace(" CHECK(", "\nCHECK (")
                            .replace(") STRICT", ")\nSTRICT")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }));
        let before = database_fingerprints(&source).unwrap();
        convert_offline(&source, &temp.path().join("new")).unwrap();
        assert_eq!(database_fingerprints(&source).unwrap(), before);
    }
}

#[test]
fn legacy_sql_lexer_keeps_comment_markers_and_escapes_inside_quoted_tokens() {
    let sql = r#"CREATE /* real comment */ TABLE "a""b--/*id*/" (
        [列 -- /*id*/] TEXT DEFAULT 'it''s -- /*value*/  spaced',
        `tick``/*id*/` BLOB DEFAULT X'2d2d'
    ); -- ignored through CR\r and until LF
    "#;
    assert_eq!(
        legacy_sql_tokens(sql).unwrap(),
        vec![
            "CREATE",
            "TABLE",
            r#""a""b--/*id*/""#,
            "(",
            "[列 -- /*id*/]",
            "TEXT",
            "DEFAULT",
            "'it''s -- /*value*/  spaced'",
            ",",
            "`tick``/*id*/`",
            "BLOB",
            "DEFAULT",
            "X'2d2d'",
            ")",
            ";"
        ]
    );
    assert_eq!(
        legacy_sql_tokens("SELECT-- comment\rstill comment\n1").unwrap(),
        ["SELECT", "1"]
    );
    assert_eq!(
        legacy_sql_tokens("SELECT 1 /* comment until EOF").unwrap(),
        ["SELECT", "1"]
    );
    for sql in [
        "SELECT 'unterminated",
        "SELECT \"unterminated",
        "SELECT [unterminated",
    ] {
        assert!(legacy_sql_tokens(sql).is_err(), "{sql}");
    }
}

#[test]
fn legacy_sql_lexer_does_not_merge_tokens_or_normalize_quoted_contents() {
    for (original, changed) in [
        ("'a b'", "'ab'"),
        ("'a--b'", "'a'"),
        ("'a/*b*/c'", "'ac'"),
        ("'a''b'", "'ab'"),
        (r#""a b""#, r#""ab""#),
        (r#""a""b""#, r#""ab""#),
        ("[a -- b]", "[a]"),
        ("`a/*b*/c`", "`ac`"),
        ("DEFAULT 30", "DEFAULT 3 0"),
        ("1.0", "1 .0"),
        ("1e+2", "1 e+2"),
        ("0x10", "0 x10"),
        ("X'ABCD'", "X 'ABCD'"),
        (">=", "> ="),
        ("||", "| |"),
        ("PRIMARY", "PRI/* boundary */MARY"),
    ] {
        assert_ne!(
            legacy_sql_tokens(original).unwrap(),
            legacy_sql_tokens(changed).unwrap(),
            "{original} versus {changed}"
        );
    }
}

#[test]
fn changed_legacy_columns_constraints_defaults_indexes_and_literals_are_refused() {
    for (original, changed) in [
        ("launch_at_startup", "launch_at_startup_changed"),
        ("BETWEEN 1 AND 3650", "BETWEEN 1 AND 3651"),
        ("DEFAULT 'system'", "DEFAULT 'dark'"),
        ("'zh-CN'", "'zh- CN'"),
        ("'zh-CN'", "'zh--CN'"),
        ("'zh-CN'", "'zh/*comment*/-CN'"),
        (
            "ON probe_records(finished_at_ms)",
            "ON probe_records(finished_at_ms DESC)",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy_with_sql(&source, 5, |sql| {
            sql.replace(original, changed)
        }));
        let before = database_fingerprints(&source).unwrap();
        let destination = temp.path().join("new");
        let error = convert_offline(&source, &destination).unwrap_err();
        assert_eq!(
            error.to_string(),
            "source schema differs from supported frozen legacy format",
            "{changed}"
        );
        assert_eq!(database_fingerprints(&source).unwrap(), before);
        assert!(destination.join("conversion-incomplete").exists());
        assert!(!destination.join("original.sqlite3").exists());
        assert!(!destination.join("conversion-verification.json").exists());
    }
}

#[test]
fn legacy_version_and_history_checks_remain_strict_after_token_comparison() {
    for change in [
        "PRAGMA user_version=1",
        "UPDATE schema_migrations SET name='unknown' WHERE position=0",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        let c = legacy(&source, 5);
        c.execute_batch(change).unwrap();
        drop(c);
        let before = database_fingerprints(&source).unwrap();
        let destination = temp.path().join("new");
        assert!(convert_offline(&source, &destination).is_err());
        assert_eq!(database_fingerprints(&source).unwrap(), before);
        assert!(!destination.join("original.sqlite3").exists());
    }
}

#[test]
fn finder_metadata_survives_conversion_replay_and_converted_host_startup() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    drop(legacy(&source, 5));
    let record = source.join("probe-records/record");
    fs::create_dir_all(&record).unwrap();
    let bytes = "{\"event\":\"record_created\",\"recordId\":\"record\",\"formatVersion\":2}\n{\"event\":\"request_started\",\"method\":\"POST\"}\n{\"event\":\"request_finished\",\"bodyComplete\":true}\n";
    fs::write(record.join("response.jsonl"), bytes).unwrap();
    let metadata = vec![0xa5; 6148];
    let relative = "probe-records/.DS_Store";
    fs::write(source.join(relative), &metadata).unwrap();
    let before = database_fingerprints(&source).unwrap();
    crate::modules::monitoring::inspect_capture_formats(&source.join("probe-records")).unwrap();
    assert_eq!(
        crate::modules::monitoring::verify_capture_history(&source.join("probe-records")).unwrap(),
        1
    );

    let base = temp.path().join("converted-host");
    fs::create_dir(&base).unwrap();
    let destination = base.join("com.relaypulse.app.dev");
    let report = convert_offline(&source, &destination).unwrap();
    assert_eq!(report.probe_packages, 1);
    assert_eq!(
        report.files[relative],
        format!("{:x}", Sha256::digest(&metadata))
    );
    assert_eq!(database_fingerprints(&source).unwrap(), before);
    assert_eq!(fs::read(source.join(relative)).unwrap(), metadata);
    assert_eq!(fs::read(destination.join(relative)).unwrap(), metadata);
    assert_eq!(
        fs::read(record.join("response.jsonl")).unwrap(),
        bytes.as_bytes()
    );
    assert_eq!(
        fs::read(destination.join("probe-records/record/response.jsonl")).unwrap(),
        bytes.as_bytes()
    );
    crate::modules::monitoring::inspect_capture_formats(&destination.join("probe-records"))
        .unwrap();
    assert_eq!(
        crate::modules::monitoring::verify_capture_history(&destination.join("probe-records"))
            .unwrap(),
        1
    );

    let (state, diagnostics) = crate::startup::initialize(Ok(
        crate::shared::persistence::test_support::environment(&base, "com.relaypulse.app.dev"),
    ));
    assert!(matches!(state, crate::startup::StartupState::Ready(_)));
    drop((state, diagnostics));
    assert_eq!(fs::read(destination.join(relative)).unwrap(), metadata);
    assert_eq!(database_fingerprints(&source).unwrap(), before);
}

#[test]
fn conversion_still_refuses_unknown_probe_files_and_finder_metadata_directories() {
    for (name, directory) in [
        ("unknown.txt", false),
        (".hidden", false),
        (".DS_Store.backup", false),
        (".DS_Store", true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("old");
        drop(legacy(&source, 5));
        let root = source.join("probe-records");
        fs::create_dir(&root).unwrap();
        let path = root.join(name);
        if directory {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, b"retained").unwrap();
        }
        let before = database_fingerprints(&source).unwrap();
        assert!(crate::modules::monitoring::inspect_capture_formats(&root).is_err());
        assert!(crate::modules::monitoring::verify_capture_history(&root).is_err());
        let target = temp.path().join("new");
        assert!(convert_offline(&source, &target).is_err(), "{name}");
        assert_eq!(database_fingerprints(&source).unwrap(), before);
        assert!(path.exists());
        assert!(!target.join("conversion-verification.json").exists());
        assert!(target.join("conversion-incomplete").is_file());
    }
}

#[cfg(unix)]
#[test]
fn conversion_refuses_linked_finder_metadata_and_retains_its_source_and_target() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("old");
    drop(legacy(&source, 5));
    let root = source.join("probe-records");
    fs::create_dir(&root).unwrap();
    let outside = temp.path().join("metadata");
    fs::write(&outside, b"retained outside source").unwrap();
    let link = root.join(".DS_Store");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let before = database_fingerprints(&source).unwrap();
    let target = temp.path().join("new");
    assert!(convert_offline(&source, &target).is_err());
    assert_eq!(database_fingerprints(&source).unwrap(), before);
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read(outside).unwrap(), b"retained outside source");
    assert!(!target.join("conversion-verification.json").exists());
    assert!(target.join("conversion-incomplete").is_file());
}

#[test]
fn format_two_is_converted_offline_with_history_preserved_and_retired_limits_removed() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source-v2");
    let target = temp.path().join("target-v3");
    fs::create_dir(&source).unwrap();
    let c = Connection::open(source.join("relaypulse.sqlite3")).unwrap();
    c.execute_batch("CREATE TABLE schema_migrations(position INTEGER PRIMARY KEY CHECK(position>=0),name TEXT NOT NULL UNIQUE,sql TEXT NOT NULL) STRICT;").unwrap();
    for (position, fragment) in BASELINE.iter().enumerate() {
        let sql = if fragment.name == "iq_test" {
            include_str!("../../../src/application/data_transfer/legacy/iq_test_format2.sql")
        } else {
            fragment.sql
        };
        c.execute_batch(sql).unwrap();
        c.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2,?3)",
            rusqlite::params![position as i64, fragment.name, sql],
        )
        .unwrap();
    }
    c.execute_batch("PRAGMA user_version=2; INSERT INTO iq_runs VALUES ('history','completed','[]','[]',120,5,1,2,1);").unwrap();
    c.pragma_update(None, "application_id", 0x52504c53_i64)
        .unwrap();
    drop(c);
    let before = fs::read(source.join("relaypulse.sqlite3")).unwrap();
    let report = convert_offline(&source, &target).unwrap();
    assert_eq!(report.format_version, 3);
    assert_eq!(fs::read(source.join("relaypulse.sqlite3")).unwrap(), before);
    let database = Database::open_directory(&target.canonicalize().unwrap(), BASELINE).unwrap();
    database
        .with_connection(|c| {
            assert_eq!(
                c.query_row("SELECT count(*) FROM iq_runs WHERE id='history'", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            let columns = c
                .prepare("PRAGMA table_info(iq_runs)")?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            assert!(!columns.iter().any(|column| column.starts_with("timeout")));
            Ok(())
        })
        .unwrap();
}
