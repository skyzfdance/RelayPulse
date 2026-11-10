CREATE TABLE operation_logs (
    id TEXT PRIMARY KEY NOT NULL,
    occurred_at_ms INTEGER NOT NULL,
    occurred_at TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('system','user')),
    actor_snapshot TEXT NOT NULL,
    action TEXT NOT NULL,
    action_group TEXT NOT NULL,
    action_label TEXT NOT NULL,
    object_snapshot TEXT NOT NULL,
    result TEXT NOT NULL CHECK(result IN ('success','failure','cancelled','blocked')),
    summary TEXT NOT NULL,
    detail_behavior TEXT NOT NULL CHECK(json_valid(detail_behavior)),
    explanation TEXT NOT NULL,
    content TEXT NOT NULL CHECK(json_valid(content))
);

CREATE INDEX operation_logs_time ON operation_logs(occurred_at_ms DESC, id ASC);
CREATE INDEX operation_logs_filters ON operation_logs(source, result, action_group);
