//! Explicit offline conversion. Source must be stopped; never invoked by startup.
use relaypulse_storage::{schema_objects, Database};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
use crate::application::composition::BASELINE;
const LEGACY_HISTORY: &str = "CREATE TABLE schema_migrations (
                position INTEGER PRIMARY KEY CHECK (position >= 0),
                name TEXT NOT NULL UNIQUE
             ) STRICT;";
const LEGACY: &[(&str, &str)] = &[
    (
        "general_configuration",
        include_str!("legacy/configuration.sql"),
    ),
    (
        "notification_configuration",
        include_str!("legacy/notifications.sql"),
    ),
    ("site_configuration", include_str!("legacy/sites.sql")),
    ("monitoring_records", include_str!("legacy/monitoring.sql")),
    (
        "operation_history",
        include_str!("legacy/operation_logs.sql"),
    ),
    ("iq_test", include_str!("legacy/iq_test.sql")),
    (
        "iq_completion_notifications",
        include_str!("legacy/iq_rule.sql"),
    ),
];
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub format_version: u8,
    pub tables: BTreeMap<String, u64>,
    pub files: BTreeMap<String, String>,
    pub source_database_files: BTreeMap<String, String>,
    pub iq_attempts: usize,
    pub probe_packages: usize,
}
fn supported(source: &Connection) -> Result<()> {
    let version: i64 = source.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 2 {
        return supported_format2(source);
    }
    if version != 0 {
        return Err("unsupported source database version".into());
    }
    let names = source
        .prepare("SELECT name FROM schema_migrations ORDER BY position")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !(5..=7).contains(&names.len()) {
        return Err("unsupported legacy baseline length".into());
    }
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(LEGACY_HISTORY)?;
    for ((_, sql), name) in LEGACY.iter().zip(&names) {
        expected.execute_batch(sql)?;
        // The exact names are read from the frozen original adapter constants below.
        if name
            != legacy_names()[expected.query_row(
                "SELECT count(*) FROM schema_migrations",
                [],
                |r| r.get::<_, i64>(0),
            )? as usize]
        {
            return Err("unknown source schema history".into());
        }
        expected.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2)",
            rusqlite::params![
                expected.query_row("SELECT count(*) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))?,
                name
            ],
        )?;
    }
    let actual = schema_objects(source)?;
    let frozen = schema_objects(&expected)?;
    if actual.len() != frozen.len() {
        return Err("source schema differs from supported frozen legacy format".into());
    }
    for ((name, sql), (expected_name, expected_sql)) in actual.iter().zip(&frozen) {
        if name != expected_name || legacy_sql_tokens(sql)? != legacy_sql_tokens(expected_sql)? {
            return Err("source schema differs from supported frozen legacy format".into());
        }
    }
    Ok(())
}

// Frozen IQ format is recognized only by the explicit offline converter.
fn supported_format2(source: &Connection) -> Result<()> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch("CREATE TABLE schema_migrations(position INTEGER PRIMARY KEY CHECK(position>=0),name TEXT NOT NULL UNIQUE,sql TEXT NOT NULL) STRICT;")?;
    for (position, fragment) in BASELINE.iter().enumerate() {
        let sql = if fragment.name == crate::modules::iq_test::SCHEMA.name {
            include_str!("legacy/iq_test_format2.sql")
        } else {
            fragment.sql
        };
        expected.execute_batch(sql)?;
        expected.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2,?3)",
            rusqlite::params![position as i64, fragment.name, sql],
        )?;
    }
    let manifest = |connection: &Connection| -> rusqlite::Result<Vec<(i64, String, String)>> {
        connection
            .prepare("SELECT position,name,sql FROM schema_migrations ORDER BY position")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect()
    };
    if source.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))? != 0x52504c53
        || schema_objects(source)? != schema_objects(&expected)?
        || manifest(source)? != manifest(&expected)?
    {
        return Err("source differs from frozen format 2".into());
    }
    Ok(())
}

// Only the offline legacy recognizer ignores comments/layout. Keep each remaining SQLite
// token verbatim: quotes, literal contents, word boundaries and compound operators matter.
// This is not semantic SQL normalization, and never rewrites the stored schema.
fn legacy_sql_tokens(sql: &str) -> Result<Vec<&str>> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0c => {
                i += 1;
                continue;
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') && bytes.get(i + 2).is_some() => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                // SQLite also treats an open block comment through EOF as trivia.
                i = (i + 2).min(bytes.len());
                continue;
            }
            _ => {}
        }
        let start = i;
        if matches!(bytes[i], b'\'' | b'"' | b'`' | b'[') {
            i = legacy_quoted_end(bytes, i)?;
        } else if matches!(bytes[i], b'x' | b'X') && bytes.get(i + 1) == Some(&b'\'') {
            // X'...' is one blob literal, distinct from the identifier X then a string.
            i = legacy_quoted_end(bytes, i + 1)?;
        } else if bytes[i].is_ascii_digit()
            || (bytes[i] == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            if bytes[i] == b'0'
                && matches!(bytes.get(i + 1), Some(b'x' | b'X'))
                && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
            {
                i += 2;
                while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                    i += 1;
                }
            } else {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'.') {
                    i += 1;
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                        i += 1;
                    }
                }
                if matches!(bytes.get(i), Some(b'e' | b'E')) {
                    let exponent =
                        i + 1 + usize::from(matches!(bytes.get(i + 1), Some(b'+' | b'-')));
                    if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                        i = exponent + 1;
                        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                            i += 1;
                        }
                    }
                }
            }
            if bytes.get(i).is_some_and(|b| legacy_identifier_byte(*b)) {
                return Err("invalid legacy schema SQL number".into());
            }
        } else if legacy_identifier_byte(bytes[i]) && bytes[i] != b'$' {
            i += 1;
            while i < bytes.len() && legacy_identifier_byte(bytes[i]) {
                i += 1;
            }
        } else if let Some(operator) = ["->>", "->", "<=", ">=", "!=", "==", "<>", "<<", ">>", "||"]
            .into_iter()
            .find(|operator| sql[i..].starts_with(operator))
        {
            i += operator.len();
        } else if b"();,+-*/%&~<>=|.".contains(&bytes[i]) {
            i += 1;
        } else {
            // Bind parameters and unknown/malformed tokens cannot belong to the frozen DDL.
            return Err("unsupported legacy schema SQL token".into());
        }
        tokens.push(&sql[start..i]);
    }
    Ok(tokens)
}
fn legacy_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | 0x80..=0xff)
}
fn legacy_quoted_end(bytes: &[u8], start: usize) -> Result<usize> {
    let quote = bytes[start];
    let delimiter = if quote == b'[' { b']' } else { quote };
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == delimiter {
            if quote != b'[' && bytes.get(i + 1) == Some(&delimiter) {
                i += 2;
                continue;
            }
            return Ok(i + 1);
        }
        i += 1;
    }
    Err("unterminated legacy schema SQL quote".into())
}

fn legacy_names() -> [&'static str; 7] {
    [
        "configuration",
        "notification_configuration",
        "site_configuration",
        "site_probe_records",
        "operation_history",
        "iq_test",
        "iq_completion_notifications",
    ]
}
fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
fn check_integrity(c: &Connection) -> Result<()> {
    if c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err("database integrity check failed".into());
    }
    if c.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err("database relationship check failed".into());
    }
    Ok(())
}
const DATABASE_FILES: &[&str] = &[
    "relaypulse.sqlite3",
    "relaypulse.sqlite3-wal",
    "relaypulse.sqlite3-shm",
    "relaypulse.sqlite3-journal",
];
fn fingerprint(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("unsupported source file".into());
    }
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn copy_verified(source: &Path, destination: &Path) -> Result<String> {
    // Check before copy, so fs::copy can never follow a source link.
    let before = fingerprint(source)?;
    fs::copy(source, destination)?;
    if fingerprint(destination)? != before || fingerprint(source)? != before {
        return Err("source file changed or copied bytes differ".into());
    }
    fs::File::open(destination)?.sync_all()?;
    Ok(before)
}
fn database_fingerprints(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for name in DATABASE_FILES {
        if fs::symlink_metadata(root.join(name)).is_ok() {
            result.insert((*name).into(), fingerprint(&root.join(name))?);
        }
    }
    Ok(result)
}
fn copy_tree(
    source: &Path,
    destination: &Path,
    relative: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(source.join(relative))? {
        let entry = entry?;
        let rel = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err("linked files are unsupported; source retained".into());
        }
        if relative.as_os_str().is_empty()
            && entry.file_name().to_str().is_some_and(|n| {
                matches!(
                    n,
                    "relaypulse.sqlite3"
                        | "relaypulse.sqlite3-wal"
                        | "relaypulse.sqlite3-shm"
                        | "relaypulse.sqlite3-journal"
                )
            })
        {
            continue;
        }
        if metadata.is_dir() {
            fs::create_dir(destination.join(&rel))?;
            copy_tree(source, destination, &rel, files)?;
        } else if metadata.is_file() {
            let digest = copy_verified(&entry.path(), &destination.join(&rel))?;
            files.insert(rel.to_string_lossy().into_owned(), digest);
        } else {
            return Err("unsupported filesystem entry".into());
        }
    }
    Ok(())
}
fn evidence(root: &Path, c: &Connection) -> Result<(usize, usize)> {
    let mut attempts = 0;
    let has_iq = c
        .prepare("SELECT name FROM sqlite_schema WHERE name='iq_entries'")?
        .exists([])?;
    if has_iq {
        let entries = c
            .prepare("SELECT id,run_id,attempt,has_raw_stream FROM iq_entries")?
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)? as usize,
                    r.get::<_, bool>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, run, attempt, has_raw) in entries {
            for part in [&id, &run] {
                if part.is_empty()
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                {
                    return Err("unsafe evidence identifier".into());
                }
            }
            let dir = root
                .join("iq-test/runs")
                .join(&run)
                .join("entries")
                .join(&id);
            for n in 1..=attempt {
                let meta_path = dir.join(format!("meta.{n}.json"));
                if !meta_path.exists() {
                    if has_raw {
                        return Err("missing IQ attempt metadata".into());
                    }
                    continue;
                }
                let meta: serde_json::Value = serde_json::from_slice(&fs::read(meta_path)?)?;
                if meta["formatVersion"] != 1
                    || meta["runId"] != run
                    || meta["entryId"] != id
                    || meta["attempt"] != n
                {
                    return Err("unsupported IQ evidence metadata".into());
                }
                let raw = fs::read(dir.join(format!("stream.{n}.sse")))?;
                let _output = String::from_utf8(fs::read(dir.join(format!("output.{n}.txt")))?)?;
                if meta["bodyComplete"] == true
                    && meta["streamBytes"].as_u64() != Some(raw.len() as u64)
                {
                    return Err("incomplete IQ evidence stream".into());
                }
                attempts += 1;
            }
        }
    }
    // The owning business parser defines legacy support, including headerless history.
    let probes = crate::modules::monitoring::verify_capture_history(&root.join("probe-records"))?;
    Ok((attempts, probes))
}
fn verify_configuration(database: std::sync::Arc<Database>) -> Result<()> {
    database.with_connection(|c| {
        let mut statement = c.prepare("SELECT detail_behavior,content FROM operation_logs")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let behavior: String = row.get(0)?;
            let content: String = row.get(1)?;
            let valid = {
                use relaypulse_operations::HistoryDecoder;
                let decoder = crate::modules::monitoring::operation_history::ProbeHistoryDecoder;
                serde_json::from_str(&behavior)
                    .and_then(|v| decoder.behavior(v))
                    .is_ok()
                    && serde_json::from_str(&content)
                        .and_then(|v| decoder.content(v))
                        .is_ok()
            };
            if !valid {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }
        Ok(())
    })?;
    crate::modules::configuration::ConfigurationService::new(
        crate::modules::configuration::SqliteSettingsStore::new(database.clone()),
    )
    .load()?;
    // Constructors are inert; readiness uses business DTO readers without starting requests.
    crate::modules::sites::SiteService::new(
        database.clone(),
        Default::default(),
        Default::default(),
    )
    .check_ready()?;
    let mut seen = std::collections::HashSet::new();
    let variables = crate::modules::iq_test::notification_definitions::VARIABLES
        .iter()
        .chain(crate::modules::monitoring::notification_definitions::VARIABLES)
        .filter(|v| seen.insert(v.key))
        .cloned()
        .collect();
    let rules = crate::modules::iq_test::notification_definitions::RULES
        .iter()
        .chain(crate::modules::monitoring::notification_definitions::RULES)
        .cloned()
        .collect();
    let catalog = relaypulse_notifications::EventCatalog::new(rules, variables)?;
    relaypulse_notifications::NotificationService::new(
        relaypulse_notifications::SqliteNotificationStore::new(database),
        catalog,
    )
    .check_ready()?;
    Ok(())
}
fn transfer_columns(connection: &Connection, table: &str) -> rusqlite::Result<String> {
    let columns = connection
        .prepare(&format!("PRAGMA main.table_info({})", quote(table)))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns
        .iter()
        .map(|column| quote(column))
        .collect::<Vec<_>>()
        .join(","))
}

/// Caller attests that the source application and all external file writers have stopped.
/// Retains a full consistent SQLite backup and all source files; never switches directories.
pub(crate) fn convert_offline(source: &Path, destination: &Path) -> Result<Verification> {
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .ok_or("destination needs an existing parent")?
        .canonicalize()?;
    let destination = parent.join(destination.file_name().ok_or("destination needs a name")?);
    if destination.exists() || destination.starts_with(&source) || source.starts_with(&destination)
    {
        return Err("destination must be a new independent directory".into());
    }
    let source_db = source.join("relaypulse.sqlite3");
    if fs::symlink_metadata(&source_db)?.file_type().is_symlink() {
        return Err("linked source database is unsupported".into());
    }
    for reserved in [
        "original.sqlite3",
        "conversion-incomplete",
        "conversion-verification.json",
        ".source-verification",
        ".original-database",
        ".target-database",
    ] {
        if source.join(reserved).exists() {
            return Err("source contains reserved conversion filename".into());
        }
    }
    fs::create_dir(&destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o700))?;
    }
    // Any failure leaves a non-ready directory and the untouched original for recovery.
    fs::write(
        destination.join("conversion-incomplete"),
        b"Do not use this directory until verification succeeds.",
    )?;
    fs::File::open(destination.join("conversion-incomplete"))?.sync_all()?;
    fs::File::open(&destination)?.sync_all()?;
    let raw_snapshot = destination.join(".original-database");
    fs::create_dir(&raw_snapshot)?;
    let source_database_files = database_fingerprints(&source)?;
    for name in source_database_files.keys() {
        copy_verified(&source.join(name), &raw_snapshot.join(name))?;
    }
    // SQLite only opens our private copy; WAL replay and SHM creation cannot affect source.
    let original = Connection::open_with_flags(
        raw_snapshot.join("relaypulse.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    supported(&original)?;
    check_integrity(&original)?;
    let backup_path = destination.join("original.sqlite3");
    {
        let mut backup = Connection::open(&backup_path)?;
        rusqlite::backup::Backup::new(&original, &mut backup)?.run_to_completion(
            256,
            std::time::Duration::from_millis(10),
            None,
        )?;
    }
    let snapshot = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    supported(&snapshot)?;
    check_integrity(&snapshot)?;
    let mut files = BTreeMap::new();
    copy_tree(&source, &destination, Path::new(""), &mut files)?;
    let (iq_attempts, probe_packages) = evidence(&destination, &snapshot)?;
    let target_database_root = destination.join(".target-database");
    fs::create_dir(&target_database_root)?;
    let database = std::sync::Arc::new(Database::open_directory(&target_database_root, BASELINE)?);
    let tables=snapshot.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name!='schema_migrations' ORDER BY name")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts = BTreeMap::new();
    database.with_connection(|c| {
  c.execute("ATTACH DATABASE ?1 AS original",[backup_path.to_string_lossy().as_ref()])?;
  c.pragma_update(None,"foreign_keys","OFF")?;
  let tx=c.transaction()?;
  for table in &tables {
   let columns = transfer_columns(&tx, table)?;
   let table=quote(table);tx.execute(&format!("DELETE FROM main.{table}"),[])?;
   tx.execute(&format!("INSERT INTO main.{table} ({columns}) SELECT {columns} FROM original.{table}"),[])?;
  }
  tx.commit()?;
  c.pragma_update(None,"foreign_keys","ON")?;
  for table in &tables {
   let columns=transfer_columns(c,table)?;let q=quote(table);let count:i64=c.query_row(&format!("SELECT count(*) FROM main.{q}"),[],|r|r.get(0))?;
   let original_count:i64=c.query_row(&format!("SELECT count(*) FROM original.{q}"),[],|r|r.get(0))?;
   let different:bool=c.query_row(&format!("SELECT EXISTS(SELECT {columns} FROM main.{q} EXCEPT SELECT {columns} FROM original.{q}) OR EXISTS(SELECT {columns} FROM original.{q} EXCEPT SELECT {columns} FROM main.{q})"),[],|r|r.get(0))?;
   if count!=original_count || different {return Err(rusqlite::Error::InvalidQuery);}
   counts.insert(table.clone(),count as u64);
  }
  c.execute_batch("DETACH DATABASE original; PRAGMA wal_checkpoint(TRUNCATE);")?;
  Ok(())
 })?;
    database.with_connection(|c| {
        if c.prepare("PRAGMA foreign_key_check")?.exists([])? {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    })?;
    verify_configuration(database.clone())?;
    crate::modules::iq_test::verify_history(database.clone(), &destination.join("iq-test"))?;
    let replayed_probes =
        crate::modules::monitoring::verify_capture_history(&destination.join("probe-records"))?;
    if replayed_probes != probe_packages {
        return Err("probe replay count differs".into());
    }
    database.with_connection(|c| {
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
    })?;
    drop(database);
    check_integrity(&Connection::open_with_flags(
        target_database_root.join("relaypulse.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)?;
    let report = Verification {
        format_version: relaypulse_storage::FORMAT_VERSION as u8,
        tables: counts,
        files,
        source_database_files,
        iq_attempts,
        probe_packages,
    };
    fs::write(
        destination.join("conversion-verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::File::open(destination.join("conversion-verification.json"))?.sync_all()?;
    if database_fingerprints(&source)? != report.source_database_files {
        return Err("source database changed during offline conversion".into());
    }
    let verify_root = destination.join(".source-verification");
    fs::create_dir(&verify_root)?;
    let mut current_files = BTreeMap::new();
    copy_tree(&source, &verify_root, Path::new(""), &mut current_files)?;
    if current_files != report.files {
        return Err("source files changed during offline conversion".into());
    }
    fs::remove_dir_all(verify_root)?;
    fs::rename(
        target_database_root.join("relaypulse.sqlite3"),
        destination.join("relaypulse.sqlite3"),
    )?;
    fs::remove_dir(&target_database_root)?;
    fs::File::open(destination.join("relaypulse.sqlite3"))?.sync_all()?;
    fs::remove_file(destination.join("conversion-incomplete"))?;
    fs::File::open(&destination)?.sync_all()?;
    Ok(report)
}
#[cfg(test)]
#[path = "../../../tests/integration/data_transfer/conversion.rs"]
mod tests;

mod cli;
pub(crate) use cli::run;
