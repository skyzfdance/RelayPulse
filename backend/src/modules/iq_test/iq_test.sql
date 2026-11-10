-- IQ 只保存脱敏的业务快照，不与站点或题库源对象建立历史外键。
CREATE TABLE iq_questions (
    id TEXT PRIMARY KEY NOT NULL,
    content TEXT NOT NULL CHECK(length(trim(content)) BETWEEN 1 AND 4096),
    reference_answer TEXT CHECK(reference_answer IS NULL OR length(reference_answer) BETWEEN 1 AND 4096),
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms)
) STRICT;
CREATE INDEX iq_questions_order ON iq_questions(enabled DESC, updated_at_ms DESC, id);
CREATE TABLE iq_runs (
    id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('running','completed','aborted')),
    questions_json TEXT NOT NULL CHECK(json_valid(questions_json) AND json_type(questions_json) = 'array'),
    participants_json TEXT NOT NULL CHECK(json_valid(participants_json) AND json_type(participants_json) = 'array'),
    started_at_ms INTEGER NOT NULL CHECK(started_at_ms >= 0),
    finished_at_ms INTEGER CHECK(finished_at_ms >= started_at_ms),
    duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0)
) STRICT;
CREATE UNIQUE INDEX iq_single_running ON iq_runs((1)) WHERE status = 'running';
CREATE INDEX iq_history_order ON iq_runs(started_at_ms DESC, id);
CREATE TABLE iq_entries (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES iq_runs(id) ON DELETE CASCADE,
    question_index INTEGER NOT NULL CHECK(question_index >= 0),
    participant_index INTEGER NOT NULL CHECK(participant_index >= 0),
    status TEXT NOT NULL CHECK(status IN ('pending','running','completed','error','timeout','aborted')),
    attempt INTEGER NOT NULL CHECK(attempt >= 0),
    started_at_ms INTEGER CHECK(started_at_ms >= 0),
    finished_at_ms INTEGER CHECK(finished_at_ms >= 0),
    first_token_ms INTEGER CHECK(first_token_ms >= 0),
    total_time_ms INTEGER CHECK(total_time_ms >= 0),
    input_tokens INTEGER CHECK(input_tokens >= 0),
    output_tokens INTEGER CHECK(output_tokens >= 0),
    error_code TEXT,
    error_message TEXT,
    has_raw_stream INTEGER NOT NULL CHECK(has_raw_stream IN (0,1)),
    UNIQUE(run_id, question_index, participant_index)
) STRICT;
