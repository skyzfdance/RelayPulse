-- 业务操作历史的独立存储，具名迁移步骤为 operation_history。
-- 与技术诊断日志分开；不建立站点、模型或探测记录外键，避免对象删除时级联丢失操作历史。
-- 名称、配置及事件内容在操作发生时保存快照，由服务在入库前完成凭据和敏感信息脱敏。
CREATE TABLE operation_logs (
    -- 操作记录标识：通常生成 UUID；启动受阻补记复用原标识，写入时按主键幂等去重。
    id TEXT PRIMARY KEY NOT NULL,
    -- 事件发生时间：Unix 时间戳，单位毫秒；用于排序及按设备本地自然日筛选，不是补记或查询时间。
    occurred_at_ms INTEGER NOT NULL,
    -- 同一发生时间的 RFC3339 字符串，当前以 UTC 输出；供前端按本地时区展示。
    occurred_at TEXT NOT NULL,
    -- 触发来源：system = 自动任务或应用生命周期，user = 用户从界面、面板等入口主动发起。
    -- 按触发入口沿任务链传递，不能因操作在 Rust 中执行就归为 system。
    source TEXT NOT NULL CHECK(source IN ('system','user')),
    -- 当时的执行者展示快照：当前为“系统任务”或“本机用户”，不使用演示身份。
    actor_snapshot TEXT NOT NULL,
    -- 原始业务动作，使用 OperationAction 的 snake_case 值；失败或取消时仍保留原动作。
    action TEXT NOT NULL,
    -- 面向列表筛选的动作分组，由 action 映射生成；例如启停、排序归 update，健康边沿归 status_change。
    action_group TEXT NOT NULL,
    -- 事件发生时的中文动作标签，与原始 action 和筛选分组分别保存。
    action_label TEXT NOT NULL,
    -- 操作对象名称或标识的脱敏快照；后续重命名或删除对象不会改写历史展示。
    object_snapshot TEXT NOT NULL,
    -- 操作执行结果：success = 成功，failure = 失败，cancelled = 已取消，blocked = 受阻。
    -- 表示本次动作的结果；正式探测任务成功完成不代表其所有模型的探测结果都正常。
    result TEXT NOT NULL CHECK(result IN ('success','failure','cancelled','blocked')),
    -- 列表使用的简短事件摘要；模型数量、耗时及逐条结果由关联探测记录展示。
    summary TEXT NOT NULL,
    -- 详情入口的 JSON 判别联合：drawer 打开详情抽屉，probe_records 携带真实 recordIds 和 availability。
    -- 关联 ID 只来自已提交的正式记录；记录过期或被删除后仍保留原 ID，由关联查询返回 missingIds。
    detail_behavior TEXT NOT NULL,
    -- 详情说明文本；与 content 一起按 logId 读取，列表查询不加载这两个字段。
    explanation TEXT NOT NULL,
    -- 已脱敏的详情 JSON，按 kind 保存对应结构：snapshot、changes、deletion、notification、
    -- cleanup、export、failure 或 probe_failure；通知保存实际成品，配置保存快照或前后差异。
    content TEXT NOT NULL
);

-- 对应默认列表顺序：发生时间倒序，同一毫秒按 ID 升序，保证分页具有稳定的次级排序。
CREATE INDEX operation_logs_time ON operation_logs(occurred_at_ms DESC, id ASC);
-- 按来源、执行结果及动作分组建立联合索引，供 SQLite 按查询条件选择；左侧前缀从 source 开始。
CREATE INDEX operation_logs_filters ON operation_logs(source, result, action_group);
