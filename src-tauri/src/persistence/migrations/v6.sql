CREATE TABLE IF NOT EXISTS credential_operations (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL CHECK (length(site_id) BETWEEN 1 AND 128),
  action TEXT NOT NULL CHECK (action IN ('create', 'replace', 'clear', 'delete')),
  status TEXT NOT NULL CHECK (
    status IN ('intent_recorded', 'credential_written', 'config_committed', 'cleanup_pending')
  ),
  new_credential_ref TEXT,
  old_credential_ref TEXT,
  non_secret_payload_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
  completed_at_ms INTEGER CHECK (
    completed_at_ms IS NULL OR completed_at_ms >= created_at_ms
  )
) STRICT;

CREATE TABLE credential_operations_v6 (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL CHECK (length(site_id) BETWEEN 1 AND 128),
  action TEXT NOT NULL CHECK (action IN ('create', 'replace', 'clear', 'delete')),
  status TEXT NOT NULL CHECK (
    status IN (
      'intent_recorded',
      'credential_written',
      'config_committed',
      'cleanup_pending',
      'recovery_failed'
    )
  ),
  new_credential_ref TEXT,
  old_credential_ref TEXT,
  non_secret_payload_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
  completed_at_ms INTEGER CHECK (
    completed_at_ms IS NULL OR completed_at_ms >= created_at_ms
  )
) STRICT;

INSERT INTO credential_operations_v6 (
  id,
  site_id,
  action,
  status,
  new_credential_ref,
  old_credential_ref,
  non_secret_payload_json,
  created_at_ms,
  updated_at_ms,
  completed_at_ms
)
SELECT
  id,
  site_id,
  action,
  status,
  new_credential_ref,
  old_credential_ref,
  non_secret_payload_json,
  created_at_ms,
  updated_at_ms,
  completed_at_ms
FROM credential_operations;

DROP TABLE credential_operations;
ALTER TABLE credential_operations_v6 RENAME TO credential_operations;

CREATE INDEX credential_operations_status_idx
  ON credential_operations (status, created_at_ms);
