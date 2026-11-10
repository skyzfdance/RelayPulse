-- 通知设置。
-- SQLite 使用 INTEGER + CHECK 表达严格的布尔值。
-- 授权状态由操作系统提供，不持久化。
CREATE TABLE notification_settings (
  -- 单例主键：固定为 1，整张表只保存一份通知设置；不是开关。
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  -- 消息通知总开关：0 = 关闭，1 = 开启；初始化为关闭，关闭后保留规则和模板配置。
  enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
) STRICT;

INSERT INTO notification_settings VALUES (1, 0);

-- 固定通知规则及其自定义模板。
-- 规则展示字段与默认模板由业务目录提供，不持久化。
CREATE TABLE notification_rules (
  -- 规则标识：probe.model_failed = 模型探测故障，probe.model_recovered = 模型探测恢复；不允许用户增删规则。
  id TEXT PRIMARY KEY NOT NULL CHECK (id IN ('probe.model_failed', 'probe.model_recovered')),
  -- 规则启用状态：0 = 停用，1 = 启用；两条规则均初始化为启用，实际发送还受总开关和系统授权限制。
  status INTEGER NOT NULL CHECK (status IN (0, 1)),
  -- 自定义模板：NULL = 使用业务目录中的默认模板；非 NULL = 自定义模板 JSON，初始化为 NULL。
  -- JSON 字段为 templateType（固定为 custom）、markerId、titleTemplate、bodyTemplate。
  -- 数据库只检查 JSON 语法；字段结构、标记、变量、长度及字符合法性由业务模块校验。
  custom_template TEXT CHECK (custom_template IS NULL OR json_valid(custom_template))
) STRICT;

INSERT INTO notification_rules VALUES ('probe.model_failed', 1, NULL), ('probe.model_recovered', 1, NULL);
