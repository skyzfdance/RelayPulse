ALTER TABLE general_settings RENAME TO general_settings_v1;

CREATE TABLE general_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  launch_at_startup INTEGER NOT NULL DEFAULT 0 CHECK (launch_at_startup IN (0, 1)),
  continue_in_background INTEGER NOT NULL DEFAULT 1 CHECK (continue_in_background IN (0, 1)),
  tray_action TEXT NOT NULL DEFAULT 'panel' CHECK (tray_action IN ('panel', 'disabled')),
  theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark')),
  language TEXT NOT NULL DEFAULT 'zh-CN' CHECK (language = 'zh-CN'),
  history_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (history_retention_days BETWEEN 1 AND 3650),
  diagnostic_capture_enabled INTEGER NOT NULL DEFAULT 1 CHECK (diagnostic_capture_enabled IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
) STRICT;

INSERT INTO general_settings (
  singleton,
  launch_at_startup,
  continue_in_background,
  tray_action,
  theme,
  language,
  history_retention_days,
  diagnostic_capture_enabled,
  updated_at_ms
)
SELECT
  singleton,
  launch_at_startup,
  continue_in_background,
  tray_action,
  theme,
  language,
  history_retention_days,
  diagnostic_capture_enabled,
  updated_at_ms
FROM general_settings_v1;

DROP TABLE general_settings_v1;
