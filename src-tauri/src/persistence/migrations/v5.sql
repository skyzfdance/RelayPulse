CREATE TABLE intelligence_ladder_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  interval_minutes INTEGER NOT NULL DEFAULT 60 CHECK (
    interval_minutes IN (30, 60, 180, 360)
  ),
  fetch_on_app_start INTEGER NOT NULL DEFAULT 0 CHECK (fetch_on_app_start IN (0, 1)),
  fetch_on_network_restore INTEGER NOT NULL DEFAULT 0 CHECK (
    fetch_on_network_restore IN (0, 1)
  ),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

INSERT INTO intelligence_ladder_settings (
  singleton, enabled, interval_minutes, fetch_on_app_start,
  fetch_on_network_restore, updated_at_ms
) VALUES (1, 0, 60, 0, 0, 0);

CREATE TABLE intelligence_ladder_snapshot (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  fetched_at_ms INTEGER CHECK (fetched_at_ms IS NULL OR fetched_at_ms >= 0),
  last_error_code TEXT CHECK (
    last_error_code IS NULL OR length(last_error_code) BETWEEN 1 AND 64
  ),
  last_error_message TEXT CHECK (
    last_error_message IS NULL OR length(last_error_message) BETWEEN 1 AND 160
  ),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  CHECK (
    (last_error_code IS NULL AND last_error_message IS NULL) OR
    (last_error_code IS NOT NULL AND last_error_message IS NOT NULL)
  )
) STRICT;

INSERT INTO intelligence_ladder_snapshot (
  singleton, fetched_at_ms, last_error_code, last_error_message, updated_at_ms
) VALUES (1, NULL, NULL, NULL, 0);

CREATE TABLE intelligence_ladder_entries (
  rank INTEGER PRIMARY KEY CHECK (rank >= 1),
  model TEXT NOT NULL CHECK (length(model) BETWEEN 1 AND 120),
  effort TEXT NOT NULL CHECK (length(effort) BETWEEN 1 AND 40),
  iq REAL NOT NULL CHECK (iq >= 0 AND iq <= 200),
  UNIQUE (model, effort)
) STRICT;
