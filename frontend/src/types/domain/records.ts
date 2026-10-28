/** 仅描述一次站点正式探测的结果；与持续监测的 HealthStatus 无关。 */
export type ProbeResult = 'success' | 'slow' | 'failure';
export type RecordTimeRange = '24h' | '7d' | '30d' | 'all';

/** 供应商明确报告的用量。缺失字段为 null，明确报告的 0 必须保留。 */
export interface ProbeTokenUsage {
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
}

/** 列表只携带摘要，不携带请求信息或响应正文。 */
export interface ProbeRecordListItem {
  id: string;
  /** 请求执行时的名称快照。 */
  siteName: string;
  modelName: string;
  /** 请求开始时间，带时区的 ISO 8601 时间字符串。 */
  probedAt: string;
  result: ProbeResult;
  resultMessage: string;
  firstTokenMs: number | null;
  durationMs: number | null;
  /** 列表只展示供应商明确上报的总 Token；未上报为 null，不补算输入输出之和。 */
  totalTokens: number | null;
}

/** 本次正式探测保存的请求信息快照，不从当前站点或模型配置补算。 */
export interface ProbeRecordRequestInfo {
  /** 实际请求端点的展示值，不包含凭据或敏感查询参数。 */
  endpoint: string | null;
  /** 执行时探针策略的名称快照。 */
  strategyLabel: string | null;
  /** 未收到 HTTP 响应时为 null。 */
  httpStatus: number | null;
  /** 供应商返回的请求 ID；与本地记录 id 分开。 */
  requestId: string | null;
}

/** 执行时冻结的慢阈值；任一实测耗时达到对应阈值即为偏慢，失败优先。 */
export interface ProbeRecordSlowThresholds {
  /** 首 Token 慢阈值，单位毫秒；未保存时为 null。 */
  firstTokenMs: number | null;
  /** 总耗时慢阈值，单位毫秒；未保存时为 null。 */
  durationMs: number | null;
}

/** 后端依据历史快照给出的比较与触发结论，前端不得自行比较耗时。 */
export interface ProbeRecordSlowCheck {
  /** 缺少实测值、阈值或未参与判定时为 null。 */
  comparison: 'gte' | 'lt' | null;
  /** 是否为本次已保存偏慢结果的触发项。 */
  triggered: boolean;
}

export interface ProbeRecordSlowEvaluation {
  status: 'evaluated' | 'skipped_failure';
  firstToken: ProbeRecordSlowCheck;
  duration: ProbeRecordSlowCheck;
}

/** 后端读取历史请求体后计算字节数；详情默认仅返回折叠标题摘要。 */
export interface ProbeRecordRequestSummary {
  method: string | null;
  /** 已保存、脱敏后的请求体展示文本的 UTF-8 字节数，不包含请求头或记录文件包装。 */
  bodyBytes: number | null;
}

/** 已保存的请求头；敏感值由后端在保存前脱敏，不向前端提供明文凭据。 */
export interface ProbeRecordRequestHeader {
  name: string;
  value: string;
}

/** 按需读取的历史请求快照，不从当前配置重建，不包含响应正文。 */
export interface ProbeRecordRequestContent extends ProbeRecordRequestSummary {
  /** 未保存为 null；明确没有请求头为 []。数组保留重复名称及快照条目顺序。 */
  headers: ProbeRecordRequestHeader[] | null;
  /** 后端从已保存的脱敏结构化内容生成的展示文本；空串与未保存的 null 分开。 */
  body: string | null;
}

export type ProbeRecordResponseKind = 'stream' | 'json' | 'other';

/** 后端从该条历史日志提取的响应摘要，不能由请求结果或当前配置推断。 */
export interface ProbeRecordResponseSummary {
  state: 'received' | 'not_received' | 'unknown';
  kind: ProbeRecordResponseKind | null;
  /** 实际接收的响应正文字节数，排除 JSONL 包装、请求、响应头及文件磁盘开销。 */
  receivedBytes: number | null;
}

export interface ProbeRecordResponseHeader {
  name: string;
  /** 后端在保存前脱敏；不包含明文凭据。 */
  value: string;
}

/** 网络接收分片，不等同于 SSE 事件或 JSONL 行。序号、时间及大小均由后端返回。 */
export interface ProbeRecordResponseChunk {
  index: number;
  elapsedMs: number | null;
  bytes: number | null;
}

export interface ProbeRecordLogFile {
  /** 后端返回的绝对路径，仅供展示；打开命令不接受此前端路径。 */
  path: string;
  canOpenDirectory: boolean;
}

/** 后端仅为已收到响应的失败探测提供；前端不按 HTTP 状态码或正文猜测失败。 */
export interface ProbeRecordErrorResponse {
  /** 已保存、脱敏后的响应体文本；未保存或无法展示为 null，明确空正文为 ''。 */
  body: string | null;
  /** 后端给出的实际接收完整性；未保存为 null，不能从文本长度推断。 */
  bodyComplete: boolean | null;
}

/** 按需读取元数据及失败时的响应体；正常响应不向页面传输合并流。 */
export interface ProbeRecordResponseContent extends ProbeRecordResponseSummary {
  /** 全部已保存的脱敏响应头，保留顺序和重复名称；前端不按键名筛选。 */
  headers: ProbeRecordResponseHeader[] | null;
  /** 未保存或无法读取分片信息时为 null，不在前端按数组长度生成计数。 */
  chunkCount: number | null;
  chunks: ProbeRecordResponseChunk[] | null;
  /** null 表示不显示响应体；是否展示完全由后端依据该次已保存的探测结果决定。 */
  errorResponse: ProbeRecordErrorResponse | null;
  logFile: ProbeRecordLogFile | null;
}

export interface OpenProbeRecordLogResult {
  /** 浏览器 mock 明确返回模拟提示；真实后端返回实际打开结果的提示。 */
  message: string;
}

/** 上半部分与折叠标题摘要；请求内容及响应元数据另行按需读取。 */
export interface ProbeRecordDetail extends Omit<ProbeRecordListItem, 'totalTokens'> {
  /** 详情需要的完整用量；没有任何已报告的用量时为 null。 */
  usage: ProbeTokenUsage | null;
  requestInfo: ProbeRecordRequestInfo;
  /** 后端明确返回的用量来源，页面不根据 usage 字段反推。 */
  usageSource: 'provider' | 'unreported';
  /** 仅用于解释已保存的结果，不读取当前配置重新判定；整份快照缺失时为 null。 */
  slowThresholds: ProbeRecordSlowThresholds | null;
  /** 后端返回的历史判定依据；快照缺失时为 null，不在前端补算。 */
  slowEvaluation: ProbeRecordSlowEvaluation | null;
  requestSummary: ProbeRecordRequestSummary;
  responseSummary: ProbeRecordResponseSummary;
}

export interface ProbeRecordOptions {
  /** 筛选目录允许包含目前没有历史记录的站点、模型。 */
  sites: { id: string; name: string }[];
  models: { name: string; siteIds: string[] }[];
}

export interface ProbeRecordsQuery {
  range: RecordTimeRange;
  siteId: string | null;
  modelName: string | null;
  result: ProbeResult | null;
  page: number;
  pageSize: number;
}

export interface TokenStatsQuery {
  range: RecordTimeRange;
  siteId: string | null;
  page: number;
  pageSize: number;
}

export interface RecordsPage<T> {
  items: T[];
  /** 筛选后、分页前的行数；统计列表中为站点数。 */
  total: number;
  /** 实际页码，从 1 开始；越过尾页时返回最后一页，空集合为 1。 */
  page: number;
  /** 全部保留记录的数量，不受当前筛选影响，用于区分空数据与无匹配。 */
  retainedCount: number;
}

export interface TokenStatsSummary {
  probeCount: number;
  /** 至少一个 usage 字段有数值的请求数量。 */
  reportedCount: number;
  /** 合计平均值的分母；不能对各站点平均值再取平均。 */
  totalReportedCount: number;
  inputTokens: number | null;
  outputTokens: number | null;
  /** 仅累加明确报告的 totalTokens；没有任何值时为 null。 */
  totalTokens: number | null;
  averageTokens: number | null;
}

export interface SiteTokenStats {
  siteId: string;
  siteName: string;
  probeCount: number;
  reportedCount: number;
  /** 平均每次的分母：明确报告 totalTokens 的请求数。 */
  totalReportedCount: number;
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  averageTokens: number | null;
}

export interface TokenStatsPage extends RecordsPage<SiteTokenStats> {
  /** 当前时间、站点筛选下的全部记录汇总，不受分页影响。 */
  summary: TokenStatsSummary;
}
