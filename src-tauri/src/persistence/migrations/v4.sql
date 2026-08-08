CREATE TABLE notification_template_overrides (
  event_type TEXT NOT NULL REFERENCES notification_rules(event_type) ON DELETE CASCADE,
  channel_id TEXT NOT NULL CHECK (length(channel_id) BETWEEN 1 AND 64),
  mode TEXT NOT NULL CHECK (mode IN ('default', 'custom')),
  marker_id TEXT NOT NULL CHECK (length(marker_id) BETWEEN 1 AND 64),
  title_template TEXT NOT NULL CHECK (length(title_template) BETWEEN 1 AND 480),
  body_template TEXT NOT NULL CHECK (length(body_template) BETWEEN 1 AND 2000),
  template_schema_version INTEGER NOT NULL CHECK (template_schema_version >= 1),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  PRIMARY KEY (event_type, channel_id)
) STRICT;
