CREATE TABLE notification_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
  notifications_enabled INTEGER NOT NULL DEFAULT 1 CHECK (notifications_enabled IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

INSERT INTO notification_settings (
  singleton, revision, notifications_enabled, updated_at_ms
) VALUES (1, 0, 1, 0);

CREATE TABLE notification_rules (
  event_type TEXT PRIMARY KEY NOT NULL CHECK (length(event_type) BETWEEN 1 AND 120),
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

CREATE TABLE notification_rule_channels (
  event_type TEXT NOT NULL REFERENCES notification_rules(event_type) ON DELETE CASCADE,
  channel_id TEXT NOT NULL CHECK (length(channel_id) BETWEEN 1 AND 64),
  PRIMARY KEY (event_type, channel_id)
) STRICT;

CREATE TABLE notification_events (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  event_type TEXT NOT NULL CHECK (length(event_type) BETWEEN 1 AND 120),
  payload_version INTEGER NOT NULL CHECK (payload_version >= 1),
  occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 120),
  body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 500),
  dedupe_key TEXT NOT NULL UNIQUE CHECK (length(dedupe_key) BETWEEN 1 AND 200),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE TABLE notification_outbox (
  event_id TEXT NOT NULL REFERENCES notification_events(id) ON DELETE CASCADE,
  channel_id TEXT NOT NULL CHECK (length(channel_id) BETWEEN 1 AND 64),
  status TEXT NOT NULL CHECK (
    status IN ('pending', 'dispatching', 'dispatched', 'failed', 'suppressed')
  ),
  attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
  next_attempt_at_ms INTEGER NOT NULL CHECK (next_attempt_at_ms >= 0),
  lease_token TEXT CHECK (lease_token IS NULL OR length(lease_token) BETWEEN 1 AND 128),
  lease_until_ms INTEGER CHECK (lease_until_ms IS NULL OR lease_until_ms >= 0),
  error_code TEXT CHECK (error_code IS NULL OR length(error_code) BETWEEN 1 AND 120),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  PRIMARY KEY (event_id, channel_id)
) STRICT;

CREATE INDEX notification_outbox_dispatch_idx
  ON notification_outbox(status, next_attempt_at_ms, created_at_ms);
