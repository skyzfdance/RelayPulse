CREATE TABLE app_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
) STRICT;

INSERT INTO app_state (singleton, revision) VALUES (1, 0);

CREATE TABLE sites (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  protocol TEXT NOT NULL CHECK (
    protocol IN ('openai-compatible', 'anthropic', 'openai-responses', 'custom-http')
  ),
  base_url TEXT NOT NULL CHECK (length(trim(base_url)) > 0),
  credential_ref TEXT UNIQUE CHECK (
    credential_ref IS NULL OR length(credential_ref) BETWEEN 1 AND 128
  ),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
  models_method TEXT NOT NULL DEFAULT 'GET' CHECK (models_method IN ('GET', 'POST')),
  models_endpoint TEXT NOT NULL DEFAULT '/models',
  probe_method TEXT NOT NULL DEFAULT 'POST' CHECK (probe_method IN ('GET', 'POST')),
  probe_endpoint TEXT NOT NULL DEFAULT '/chat/completions',
  content_type TEXT NOT NULL DEFAULT 'application/json',
  auth_preset TEXT NOT NULL DEFAULT 'bearer' CHECK (
    auth_preset IN ('bearer', 'x-api-key', 'api-key', 'none')
  ),
  stream_enabled INTEGER NOT NULL DEFAULT 1 CHECK (stream_enabled IN (0, 1)),
  probe_interval_minutes INTEGER DEFAULT 10 CHECK (
    probe_interval_minutes IS NULL OR probe_interval_minutes BETWEEN 1 AND 1440
  ),
  first_token_timeout_ms INTEGER NOT NULL DEFAULT 8000 CHECK (
    first_token_timeout_ms BETWEEN 1000 AND 120000
  ),
  total_timeout_ms INTEGER NOT NULL DEFAULT 30000 CHECK (
    total_timeout_ms BETWEEN 1000 AND 300000 AND total_timeout_ms >= first_token_timeout_ms
  ),
  slow_first_token_ms INTEGER NOT NULL DEFAULT 2000 CHECK (
    slow_first_token_ms > 0 AND slow_first_token_ms < first_token_timeout_ms
  ),
  slow_total_ms INTEGER NOT NULL DEFAULT 10000 CHECK (
    slow_total_ms > 0 AND slow_total_ms < total_timeout_ms
  ),
  failure_threshold INTEGER NOT NULL DEFAULT 2 CHECK (failure_threshold BETWEEN 1 AND 10),
  recovery_threshold INTEGER NOT NULL DEFAULT 2 CHECK (recovery_threshold BETWEEN 1 AND 10),
  lifecycle_state TEXT NOT NULL DEFAULT 'ready' CHECK (
    lifecycle_state IN ('ready', 'credential_pending', 'cleanup_pending', 'delete_pending')
  ),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
) STRICT;

CREATE UNIQUE INDEX sites_sort_order_unique ON sites (sort_order);

CREATE TABLE models (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL REFERENCES sites (id) ON DELETE CASCADE,
  model_key TEXT NOT NULL CHECK (length(trim(model_key)) > 0),
  display_name TEXT,
  source TEXT NOT NULL CHECK (source IN ('discovered', 'manual')),
  discovery_status TEXT NOT NULL DEFAULT 'available' CHECK (
    discovery_status IN ('available', 'missing')
  ),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
  UNIQUE (id, site_id),
  UNIQUE (site_id, model_key),
  UNIQUE (site_id, sort_order)
) STRICT;

CREATE TABLE probe_policies (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL REFERENCES sites (id) ON DELETE CASCADE,
  model_id TEXT,
  mode TEXT NOT NULL CHECK (
    mode IN ('inherit', 'disabled-thinking', 'minimum-reasoning')
  ),
  max_output_tokens INTEGER NOT NULL CHECK (max_output_tokens BETWEEN 1 AND 4096),
  prompt TEXT NOT NULL CHECK (length(prompt) BETWEEN 1 AND 4096),
  custom_endpoint TEXT,
  custom_method TEXT CHECK (custom_method IS NULL OR custom_method IN ('GET', 'POST')),
  custom_stream_enabled INTEGER CHECK (custom_stream_enabled IS NULL OR custom_stream_enabled IN (0, 1)),
  created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
  FOREIGN KEY (model_id, site_id) REFERENCES models (id, site_id) ON DELETE CASCADE
) STRICT;

CREATE UNIQUE INDEX probe_policies_site_default_unique
  ON probe_policies (site_id)
  WHERE model_id IS NULL;

CREATE UNIQUE INDEX probe_policies_model_unique
  ON probe_policies (model_id)
  WHERE model_id IS NOT NULL;

CREATE TABLE custom_probe_parameters (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  policy_id TEXT NOT NULL REFERENCES probe_policies (id) ON DELETE CASCADE,
  parameter_key TEXT NOT NULL CHECK (length(trim(parameter_key)) > 0),
  value_type TEXT NOT NULL CHECK (value_type IN ('string', 'boolean', 'number', 'json')),
  value_json TEXT NOT NULL,
  sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
  UNIQUE (policy_id, parameter_key),
  UNIQUE (policy_id, sort_order)
) STRICT;

CREATE TABLE site_headers (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL REFERENCES sites (id) ON DELETE CASCADE,
  header_name TEXT NOT NULL CHECK (length(trim(header_name)) > 0),
  value_template TEXT NOT NULL,
  sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
  UNIQUE (site_id, header_name),
  UNIQUE (site_id, sort_order)
) STRICT;

CREATE TABLE probe_results (
  id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 128),
  site_id TEXT NOT NULL REFERENCES sites (id) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  run_kind TEXT NOT NULL CHECK (run_kind IN ('scheduled', 'manual', 'trial')),
  started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
  finished_at_ms INTEGER CHECK (
    finished_at_ms IS NULL OR finished_at_ms >= started_at_ms
  ),
  status TEXT NOT NULL CHECK (
    status IN (
      'healthy',
      'slow',
      'probing',
      'rate_limited',
      'auth_error',
      'model_unavailable',
      'timeout',
      'protocol_error',
      'validation_failed',
      'upstream_error',
      'network_error',
      'cancelled',
      'unknown'
    )
  ),
  response_headers_ms INTEGER CHECK (response_headers_ms IS NULL OR response_headers_ms >= 0),
  first_token_ms INTEGER CHECK (first_token_ms IS NULL OR first_token_ms >= 0),
  total_time_ms INTEGER CHECK (total_time_ms IS NULL OR total_time_ms >= 0),
  http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
  provider_model TEXT,
  finish_reason TEXT,
  error_code TEXT,
  error_message TEXT,
  input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
  output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
  total_tokens INTEGER CHECK (total_tokens IS NULL OR total_tokens >= 0),
  usage_reported INTEGER NOT NULL DEFAULT 0 CHECK (usage_reported IN (0, 1)),
  ended_normally INTEGER NOT NULL DEFAULT 0 CHECK (ended_normally IN (0, 1)),
  retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
  request_id TEXT,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  UNIQUE (id, model_id),
  FOREIGN KEY (model_id, site_id) REFERENCES models (id, site_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX probe_results_started_at_idx ON probe_results (started_at_ms DESC, id DESC);
CREATE INDEX probe_results_site_started_idx ON probe_results (site_id, started_at_ms DESC, id DESC);
CREATE INDEX probe_results_model_started_idx ON probe_results (model_id, started_at_ms DESC, id DESC);

CREATE TABLE probe_captures (
  probe_result_id TEXT PRIMARY KEY NOT NULL REFERENCES probe_results (id) ON DELETE CASCADE,
  request_headers_json TEXT NOT NULL DEFAULT '[]',
  request_body TEXT NOT NULL DEFAULT '',
  request_captured_bytes INTEGER NOT NULL DEFAULT 0 CHECK (request_captured_bytes >= 0),
  request_limit_bytes INTEGER NOT NULL DEFAULT 32768 CHECK (request_limit_bytes BETWEEN 0 AND 32768),
  request_truncated INTEGER NOT NULL DEFAULT 0 CHECK (request_truncated IN (0, 1)),
  response_mode TEXT CHECK (response_mode IS NULL OR response_mode IN ('stream', 'synchronous')),
  response_content_type TEXT,
  response_headers_json TEXT NOT NULL DEFAULT '[]',
  response_body TEXT NOT NULL DEFAULT '',
  response_chunks_json TEXT NOT NULL DEFAULT '[]',
  response_captured_bytes INTEGER NOT NULL DEFAULT 0 CHECK (response_captured_bytes >= 0),
  response_limit_bytes INTEGER NOT NULL DEFAULT 65536 CHECK (response_limit_bytes BETWEEN 0 AND 65536),
  response_truncated INTEGER NOT NULL DEFAULT 0 CHECK (response_truncated IN (0, 1)),
  CHECK (request_captured_bytes <= request_limit_bytes),
  CHECK (response_captured_bytes <= response_limit_bytes)
) STRICT;

CREATE TABLE latest_model_status (
  model_id TEXT PRIMARY KEY NOT NULL REFERENCES models (id) ON DELETE CASCADE,
  probe_result_id TEXT REFERENCES probe_results (id) ON DELETE SET NULL,
  status TEXT NOT NULL CHECK (
    status IN (
      'healthy',
      'slow',
      'probing',
      'rate_limited',
      'auth_error',
      'model_unavailable',
      'timeout',
      'protocol_error',
      'validation_failed',
      'upstream_error',
      'network_error',
      'cancelled',
      'unknown'
    )
  ),
  consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
  consecutive_successes INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_successes >= 0),
  next_probe_at_ms INTEGER,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

CREATE TRIGGER latest_model_status_probe_model_insert
BEFORE INSERT ON latest_model_status
WHEN NEW.probe_result_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM probe_results
    WHERE id = NEW.probe_result_id AND model_id = NEW.model_id
  )
BEGIN
  SELECT RAISE(ABORT, 'latest model status probe/model mismatch');
END;

CREATE TRIGGER latest_model_status_probe_model_update
BEFORE UPDATE OF probe_result_id, model_id ON latest_model_status
WHEN NEW.probe_result_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM probe_results
    WHERE id = NEW.probe_result_id AND model_id = NEW.model_id
  )
BEGIN
  SELECT RAISE(ABORT, 'latest model status probe/model mismatch');
END;

CREATE TABLE general_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  launch_at_startup INTEGER NOT NULL DEFAULT 0 CHECK (launch_at_startup IN (0, 1)),
  continue_in_background INTEGER NOT NULL DEFAULT 1 CHECK (continue_in_background IN (0, 1)),
  tray_action TEXT NOT NULL DEFAULT 'panel' CHECK (tray_action = 'panel'),
  theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark')),
  language TEXT NOT NULL DEFAULT 'zh-CN' CHECK (language = 'zh-CN'),
  history_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (history_retention_days BETWEEN 1 AND 3650),
  diagnostic_capture_enabled INTEGER NOT NULL DEFAULT 1 CHECK (diagnostic_capture_enabled IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

INSERT INTO general_settings (singleton, updated_at_ms) VALUES (1, 0);

CREATE TABLE credential_operations (
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

CREATE INDEX credential_operations_status_idx ON credential_operations (status, created_at_ms);
