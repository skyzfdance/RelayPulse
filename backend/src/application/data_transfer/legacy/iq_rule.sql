-- 追加具名步骤，保留已有探针规则、总开关及自定义模板。
ALTER TABLE notification_rules RENAME TO notification_rules_before_iq;
CREATE TABLE notification_rules (
    id TEXT PRIMARY KEY NOT NULL CHECK(id IN ('probe.model_failed','probe.model_recovered','iq.run_completed')),
    status INTEGER NOT NULL CHECK(status IN (0,1)),
    custom_template TEXT CHECK(custom_template IS NULL OR json_valid(custom_template))
) STRICT;
INSERT INTO notification_rules SELECT id,status,custom_template FROM notification_rules_before_iq;
DROP TABLE notification_rules_before_iq;
INSERT INTO notification_rules VALUES ('iq.run_completed',1,NULL);
