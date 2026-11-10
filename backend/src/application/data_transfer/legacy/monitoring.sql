-- 正式监测的完整初始结构，具名步骤 site_probe_records。
-- 本轮按改版后的建表基线定义全部字段，不保留开发中间版本的增量 SQL。
-- 当前状态由 monitoring/sqlite.rs 维护，不属于历史查询与清理的删除范围。
-- 只在正式请求完成并提交时新增或更新；尚未提交过结果的模型没有状态行。
CREATE TABLE probe_states (
 -- 模型内部标识：一模型最多一条当前状态；删除模型时级联删除，与远端模型名称不同。
 model_id TEXT PRIMARY KEY NOT NULL REFERENCES site_models(id) ON DELETE CASCADE,
 -- 当前稳定健康状态：healthy = 正常，warning = 偏慢，error = 异常，muted = 等待探测。
 -- 由连续失败/恢复阈值推进，不等于最近一次请求的成功或失败；未达到阈值时可保留此前状态。
 health TEXT NOT NULL CHECK(health IN ('healthy','warning','error','muted')),
 -- 最近一次已提交正式请求的首段非空思考或回答文本耗时，单位毫秒；自定义请求、非流式或未收到此类文本时为 NULL。
 first_token_ms INTEGER CHECK(first_token_ms IS NULL OR first_token_ms>=0),
 -- 最近一次已提交正式请求的总耗时，单位毫秒；成功或失败均保存，不是慢阈值或超时配置。
 duration_ms INTEGER NOT NULL CHECK(duration_ms>=0),
 -- 当前连续失败次数：成功后清零，达到站点 failureThreshold 时转红；清理历史不会清除此计数。
 failures INTEGER NOT NULL CHECK(failures>=0),
 -- 当前连续成功次数：失败后清零，红灯后达到 recoveryThreshold 才恢复；偏慢但成功也计入。
 successes INTEGER NOT NULL CHECK(successes>=0),
 -- 最近一次已提交正式请求的完成时间：Unix 时间戳，单位毫秒；用于计算下一次周期探测是否到期。
 finished_at_ms INTEGER NOT NULL CHECK(finished_at_ms>=0)
) STRICT;

-- 每次正式探测一条历史；站点/模型关联删除时级联移除。
-- 执行上下文与供应商用量直接保存在本表，完整正文保存在文件。
-- 只保存站点正式手动/周期探测；草稿试探、IQ 预检和 IQ 正式调用均不写入此表。
CREATE TABLE probe_records (
 -- 本地记录标识：每次正式请求生成 UUID，同时用于关联该次过程文件目录；不是供应商请求 ID。
 id TEXT PRIMARY KEY NOT NULL,
 -- 所属站点内部标识：与 model_id 组成外键，保证记录中的模型确实属于此站点。
 site_id TEXT NOT NULL,
 -- 所属模型内部标识：用于关联配置及按模型查询历史，不是发送给远端的模型名称。
 model_id TEXT NOT NULL,
 -- 探测来源：manual = 用户触发的单模型或批量探测，scheduled = 周期自动探测。
 run_kind TEXT NOT NULL CHECK(run_kind IN ('manual','scheduled')),
 -- 本次请求是否成功：0 = 失败，1 = 成功；偏慢仍为成功，与当前稳定健康状态分开表达。
 success INTEGER NOT NULL CHECK(success IN (0,1)),
 -- 实际 HTTP 状态码：未收到响应头时为 NULL，例如连接失败或响应头到达前超时。
 status_code INTEGER,
 -- 本次请求的首段非空思考或回答文本耗时，单位毫秒；自定义请求、非流式或没有此类文本时为 NULL。
 first_token_ms INTEGER,
 -- 本次请求总耗时，单位毫秒；失败也记录，与首 Token 耗时分别保存。
 duration_ms INTEGER NOT NULL,
 -- 受控失败类型的 JSON 字符串："http"、"network"、"timeout" 或 "invalid_response"；成功时为 NULL。
 -- 不保存底层错误原文、完整响应或凭据；错误展示文案由应用按类型生成。
 failure TEXT,
 -- 本次正式请求的完成时间：Unix 时间戳，单位毫秒；历史排序及保留期清理以此为准。
 finished_at_ms INTEGER NOT NULL,
 -- 执行时的脱敏站点名称快照：之后修改站点名称不会改写已有历史。
 site_name TEXT,
 -- 执行时的脱敏模型名称快照：表示当次实际使用的远端模型名称，与 model_id 分开保存。
 model_name TEXT,
 -- 本次使用的协议：OpenAI 兼容、OpenAI Responses、Anthropic 或自定义 HTTP。
 protocol TEXT CHECK(protocol IN ('openai-compatible','openai-responses','anthropic','custom-http')),
 -- 建立本次执行上下文的时间：Unix 时间戳，单位毫秒；早于网络请求与结果提交。
 started_at_ms INTEGER CHECK(started_at_ms IS NULL OR started_at_ms>=0),
 -- 本次实际请求方法：当前所有探测均为 POST。
 request_method TEXT CHECK(request_method='POST'),
 -- 本次请求的脱敏完整端点：不保留查询参数、片段或已知凭据；请求头和完整请求正文不进 SQLite。
 request_endpoint TEXT,
 -- 脱敏策略快照 JSON：source 为 site_default、model_policy 或 custom_request。
 -- 同时保存配置的 maxTokens 及解析继承/默认值后的 parameters 参数行；自定义请求的 maxTokens 为 null。
 -- 参数行包含 key、文本 value 和 type；完整实际请求体由过程文件保存，不在此字段重建。
 policy_json TEXT CHECK(policy_json IS NULL OR json_valid(policy_json)),
 -- 当次首 Token 慢阈值，单位毫秒，范围 1000–100000；用于解释单次偏慢结果，不是请求超时。
 slow_first_token_ms INTEGER CHECK(slow_first_token_ms BETWEEN 1000 AND 100000),
 -- 当次总耗时慢阈值，单位毫秒，范围 1000–500000；用于解释单次偏慢结果，不是请求超时。
 slow_total_ms INTEGER CHECK(slow_total_ms BETWEEN 1000 AND 500000),
 -- 本次结果：succeeded = 成功且未达到慢阈值，slow = 成功但达到任一慢阈值，failed = 请求失败。
 -- 独立于 probe_states 中经过连续阈值判断的当前健康状态；没有首 Token 时只判断总耗时慢阈值。
 result_kind TEXT CHECK(result_kind IN ('succeeded','slow','failed')),
 -- 供应商响应头中的请求 ID，保存前脱敏；未提供时为 NULL，与本地记录 id 无关。
 request_id TEXT,
 -- 供应商实际报告的输入 Token 数；累计事件按字段更新，不重复相加；缺失为 NULL，报告 0 则保存 0。
 input_tokens INTEGER CHECK(input_tokens IS NULL OR input_tokens>=0),
 -- 供应商实际报告的输出 Token 数；缺失为 NULL；自定义 HTTP 不猜测其响应中的用量字段。
 output_tokens INTEGER CHECK(output_tokens IS NULL OR output_tokens>=0),
 -- 供应商实际报告的总 Token 数；即使已知输入和输出，供应商未报告总量时也保持 NULL。
 total_tokens INTEGER CHECK(total_tokens IS NULL OR total_tokens>=0),
 -- 按响应 Content-Type 识别的种类：event_stream = SSE，json = application/json，other = 其他。
 -- 未收到响应头时为 NULL，不依据请求的 stream 参数推定响应种类。
 response_kind TEXT CHECK(response_kind IN ('event_stream','json','other')),
 -- 实际收到的响应正文字节数，在脱敏和格式整理前统计；不是过程文件的磁盘大小。
 received_bytes INTEGER CHECK(received_bytes IS NULL OR received_bytes>=0),
 -- 是否完整读到响应正文 EOF：0 = 未完整读取，1 = 已完整读取；与 HTTP 成功和协议内容有效性无关。
 body_complete INTEGER CHECK(body_complete IN (0,1)),
 -- 本次是否启用过程文件：0 = 不记录，1 = 记录；不表示文件已经完整写入或发布成功。
 -- 文件按 recordId 定位，是否已发布以 <recordId>/ 与 <recordId>.partial/ 为准。
 -- 文件系统与 SQLite 不共享事务，不在库中伪造已发布标记。
 capture_enabled INTEGER CHECK(capture_enabled IN (0,1)),
 -- 关联站点与模型：删除任一所属配置时级联删除历史，当前状态由上表自己的外键清理。
 FOREIGN KEY(site_id,model_id) REFERENCES site_models(site_id,id) ON DELETE CASCADE
) STRICT;

-- 加速按完成时间查询历史和删除超过保留期的记录。
CREATE INDEX probe_records_time ON probe_records(finished_at_ms);
-- 加速指定模型的历史查询及按完成时间排序。
CREATE INDEX probe_records_model ON probe_records(model_id,finished_at_ms);
-- 加速指定站点的历史查询及按完成时间排序，不涉及 IQ 历史集合。
CREATE INDEX probe_records_site_time ON probe_records(site_id,finished_at_ms);
