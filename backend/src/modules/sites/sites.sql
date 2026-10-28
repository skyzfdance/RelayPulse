-- 站点基本信息、高级连接设置与默认探针策略。
-- SQLite 使用 INTEGER + CHECK 表达严格的布尔值，使用 TEXT 保存 JSON。
-- 数据库检查 JSON 语法；字段结构、端点、请求头及数值范围由站点业务模块校验。
-- 站点配置与凭据在同一事务保存；不依赖旧 snapshot/revision 表或旧迁移。
CREATE TABLE sites (
  -- 站点内部标识：创建时生成 UUID，用于关联模型、凭据与正式探测记录。
  id TEXT PRIMARY KEY NOT NULL,
  -- 站点展示名称：业务层去除首尾空白，不能为空；不同站点允许同名。
  name TEXT NOT NULL CHECK(length(trim(name)) > 0),
  -- 请求协议：OpenAI 兼容、OpenAI Responses、Anthropic 或自定义 HTTP。
  protocol TEXT NOT NULL CHECK(protocol IN ('openai-compatible','openai-responses','anthropic','custom-http')),
  -- 站点 Base URL：请求地址的基础部分，格式及协议由业务层校验。
  base_url TEXT NOT NULL,
  -- 小面板展示及自动探测参与开关：0 = 不展示且不自动探测，1 = 展示并允许自动探测。
  -- 自动探测还要求通用设置 continue_in_background 和对应模型 enabled 均开启；不影响同步或手动探测。
  enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
  -- 站点显示顺序：从 1 开始，数值越小越靠前；新站点追加到末尾，排序操作按完整列表重写。
  priority INTEGER NOT NULL CHECK(priority > 0),
  -- 高级连接设置 JSON：modelsPath、probePath 为目录与常规探测端点，headers 为请求头数组。
  -- intervalSeconds 为探测间隔；slowFirstTokenSeconds、slowTotalSeconds 为慢响应黄灯阈值，单位均为秒。
  -- failureThreshold、recoveryThreshold 为连续失败转红、连续成功恢复的次数；慢阈值不作为请求超时。
  connection TEXT NOT NULL CHECK(json_valid(connection)),
  -- 站点默认探针策略 JSON：必须存在，供没有独立设置的模型继承。
  -- 字段为 prompt、maxTokens（1–128 的整数）和 parameters；参数行由 key、value、type 组成。
  -- value 保存文本，type 可为 string、boolean、number、json，执行前再转换为对应类型。
  policy TEXT NOT NULL CHECK(json_valid(policy)),
  -- 配置修订号：初始化为 0，保存配置、切换启用状态或修改模型时由业务层递增。
  -- 同步检查记住此值，确认写入前再次比较，用于拒绝检查后配置已变化的过期差异；不返回前端。
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0)
) STRICT;

-- 站点凭据：与配置共用当前环境的本地 SQLite，使用独立表隔离读取接口。
-- 数据库存储明文凭据；文件权限由 persistence 管理，此表不提供加密。
-- 密钥不进入读取 DTO、日志或诊断导出，前端只收到 credentialConfigured。
CREATE TABLE site_credentials (
  -- 所属站点：同时作为主键，一站点最多一条凭据；删除站点时自动删除凭据。
  site_id TEXT PRIMARY KEY NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
  -- 非空访问密钥：keep 保留已有值，replace 校验新值后在保存基本信息的事务中替换。
  secret TEXT NOT NULL CHECK(length(secret) > 0)
) STRICT;

-- 站点下的模型目录与模型独立探针配置。
-- 健康状态、耗时和正式历史由 monitoring 的表保存，不存入本表。
CREATE TABLE site_models (
  -- 模型内部标识：创建时生成 UUID；与远端模型名称不同，删除或更新模型时使用此标识。
  id TEXT PRIMARY KEY NOT NULL,
  -- 所属站点：删除站点时自动删除其模型，关联的正式状态和历史通过各自外键继续级联删除。
  site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
  -- 实际模型名称：同步目录时按此名称匹配，执行请求时由系统注入；业务层去除首尾空白。
  name TEXT NOT NULL CHECK(length(trim(name)) > 0),
  -- 站点内显示顺序：从 0 开始，新模型追加到末尾；删除后允许留有空位，读取按 position、id 排序。
  position INTEGER NOT NULL CHECK(position >= 0),
  -- 模型创建时间：Unix 时间戳，单位毫秒；尚无正式结果时用于计算首次周期探测的到期时间。
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  -- 批量及周期探测开关：0 = 不参与，1 = 参与；业务层新增模型时设为 1。
  -- 明确指定该模型的单次探测不受此开关限制，与站点的小面板展示开关含义不同。
  enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
  -- 模型独立常规策略 JSON：字段结构同 sites.policy；非 NULL 时覆盖站点默认策略。
  policy TEXT CHECK(policy IS NULL OR json_valid(policy)),
  -- 模型自定义请求 JSON：字段为 endpoint、parameters；参数行结构同常规策略。
  -- 使用 POST，同源端点由业务层校验，继承站点凭据与请求头；非 NULL 时使用该请求。
  custom_request TEXT CHECK(custom_request IS NULL OR json_valid(custom_request)),
  -- 两种独立配置互斥：不得同时非 NULL；两者都为 NULL 时继承站点默认策略。
  CHECK(policy IS NULL OR custom_request IS NULL),
  -- 同站点模型名称唯一，避免重复添加或同步产生同名模型；不同站点允许同名。
  UNIQUE(site_id, name),
  -- 供正式记录的 (site_id, model_id) 组合外键引用，保证记录中的模型确实属于指定站点。
  UNIQUE(site_id, id)
) STRICT;

-- 加速按站点读取模型列表并按显示顺序排列；该索引不限制 position 唯一。
CREATE INDEX site_models_order ON site_models(site_id, position);
