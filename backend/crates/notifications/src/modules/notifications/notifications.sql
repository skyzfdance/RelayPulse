-- Generic notification configuration. Event defaults belong to their providers.
CREATE TABLE notification_settings (singleton INTEGER PRIMARY KEY CHECK(singleton=1), enabled INTEGER NOT NULL CHECK(enabled IN (0,1))) STRICT;
INSERT INTO notification_settings VALUES (1,0);
CREATE TABLE notification_rules (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 200),
 status INTEGER NOT NULL CHECK(status IN (0,1)),
 custom_template TEXT CHECK(custom_template IS NULL OR json_valid(custom_template))
) STRICT;
