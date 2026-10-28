import type {
  ProbeRecordErrorResponse,
  ProbeRecordListItem,
  ProbeRecordRequestContent,
  ProbeRecordRequestInfo,
  ProbeRecordResponseContent,
  ProbeRecordResponseHeader,
  ProbeRecordResponseKind,
  ProbeRecordSlowThresholds,
  ProbeResult,
  ProbeTokenUsage,
} from '@/types/domain/records';

/** 模拟后端的内部历史数据；站点关联和完整用量不直接作为列表 DTO 返回。 */
export interface MockProbeRecord extends Omit<ProbeRecordListItem, 'totalTokens'> {
  siteId: string;
  usage: ProbeTokenUsage | null;
}

interface FailureResponseFixture extends ProbeRecordErrorResponse {
  httpStatus: number;
  kind: ProbeRecordResponseKind;
  resultMessage: string;
}

/** 模拟后端已判定的失败及其历史响应；不把任意 JSON code 当成 HTTP 状态码。 */
function failureResponseFixture(recordId: string): FailureResponseFixture {
  const examples: Record<string, Partial<FailureResponseFixture>> = {
    'probe-003': {
      httpStatus: 529,
      resultMessage: '供应商暂时过载（HTTP 529）',
      body: '{"error":{"message":"Provider is overloaded","type":"overloaded_error"}}',
    },
    'probe-004': {
      httpStatus: 400,
      resultMessage: '请求参数无效（HTTP 400）',
      body: '{"error":{"message":"Invalid model parameter","type":"invalid_request_error"}}',
    },
    'probe-007': {
      httpStatus: 200,
      resultMessage: 'HTTP 请求成功，但供应商返回了错误内容',
      body: '{"error":{"message":"The provider rejected this request","type":"api_error","code":10042}}',
    },
    'probe-017': { body: '' },
    'probe-022': { body: null, bodyComplete: null },
    'probe-027': {
      body: '{"error":{"message":"Service temporarily unavailable',
      bodyComplete: false,
    },
    'probe-032': {
      httpStatus: 302,
      kind: 'other',
      resultMessage: '请求被重定向（HTTP 302），未取得有效探测响应',
      body: '<html><body>Moved temporarily</body></html>',
    },
  };
  return {
    httpStatus: 503,
    kind: 'json',
    resultMessage: '模型服务返回了服务器错误（HTTP 503）',
    body: '{"error":{"message":"Service temporarily unavailable","type":"api_error"}}',
    bodyComplete: true,
    ...examples[recordId],
  };
}

/** 独立的正式历史样例，不读取、重算或改写站点的当前健康状态。 */
export function createProbeRecordFixtures(now = Date.now()): MockProbeRecord[] {
  const sites = [
    { id: 'site-1', name: 'ai.input.im', models: ['GPT-6 Astra', 'GPT-5.6 Sol'] },
    { id: 'site-2', name: 'doudui.cc', models: ['GPT-6.1 Sol', 'GPT-6 Astra', 'GPT-5.6 Sol'] },
    { id: 'site-3', name: 'Anthropic', models: ['Claude Sonnet', 'Claude Opus'] },
  ];
  const messages: Record<ProbeResult, string> = {
    success: '请求成功',
    slow: '响应偏慢',
    failure: '模型服务返回了服务器错误（HTTP 503）',
  };
  const records = Array.from({ length: 48 }, (_, index): MockProbeRecord => {
    const site = sites[index % sites.length];
    let ageMinutes = 1 + index * 42;
    if (index >= 30) ageMinutes = (1.5 + (index - 30) * 0.45) * 1440;
    if (index >= 42) ageMinutes = (9 + (index - 42) * 3.5) * 1440;
    let result: ProbeResult = 'success';
    if (index % 7 === 0) result = 'slow';
    if (index % 5 === 1) result = 'failure';
    let usage: ProbeTokenUsage | null = {
      inputTokens: 320 + index * 83,
      outputTokens: 5 + (index % 11),
      totalTokens: 325 + index * 83 + (index % 11),
    };
    if (site.id === 'site-2' || result === 'failure' || index % 11 === 5) usage = null;
    if (index === 9) usage = { inputTokens: 0, outputTokens: 0, totalTokens: 0 };
    if (index === 8) usage = { inputTokens: 640, outputTokens: null, totalTokens: null };
    return {
      id: `probe-${String(index + 1).padStart(3, '0')}`,
      siteId: site.id,
      siteName: site.name,
      modelName: site.models[index % site.models.length],
      probedAt: new Date(now - ageMinutes * 60_000).toISOString(),
      result,
      resultMessage: messages[result],
      firstTokenMs: result === 'failure' ? null : 220 + index * 117,
      durationMs: 700 + index * 243,
      usage,
    };
  });
  Object.assign(records[0], {
    firstTokenMs: 38_740,
    durationMs: 39_870,
    usage: { inputTokens: 4389, outputTokens: 5, totalTokens: 4394 },
  });
  for (let index = 1; index <= 3; index++) {
    Object.assign(records[index], {
      siteId: sites[1].id,
      siteName: sites[1].name,
      modelName: sites[1].models[index - 1],
      probedAt: new Date(now - (4 + index) * 60_000).toISOString(),
      result: 'failure',
      resultMessage: messages.failure,
      firstTokenMs: null,
      durationMs: [475, 483, 17_020][index - 1],
      usage: null,
    });
  }
  Object.assign(records[11], {
    result: 'failure',
    resultMessage: '连接超时，未取得响应耗时与用量',
    firstTokenMs: null,
    durationMs: null,
    usage: null,
  });
  for (const record of records) {
    if (record.result === 'failure' && record.durationMs !== null)
      record.resultMessage = failureResponseFixture(record.id).resultMessage;
  }
  return records;
}

/** 只生成本模块的展示样例，不读取现有站点配置或发出真实请求。 */
export function createProbeRecordRequestInfoFixture(
  record: MockProbeRecord,
): ProbeRecordRequestInfo {
  const requests: Record<string, { endpoint: string; strategyLabel: string }> = {
    'site-1': { endpoint: 'https://input.codes/v1/responses', strategyLabel: '低推理' },
    'site-2': { endpoint: 'https://doudui.cc/v1/responses', strategyLabel: '低推理' },
    'site-3': { endpoint: 'https://api.anthropic.com/v1/messages', strategyLabel: '标准探针' },
  };
  const receivedResponse = record.result !== 'failure' || record.durationMs !== null;
  let httpStatus: number | null = null;
  if (receivedResponse)
    httpStatus = record.result === 'failure' ? failureResponseFixture(record.id).httpStatus : 200;
  return {
    endpoint: requests[record.siteId]?.endpoint ?? null,
    strategyLabel: requests[record.siteId]?.strategyLabel ?? null,
    httpStatus,
    requestId: receivedResponse ? `mock-request-${record.id}` : null,
  };
}

/** 独立的历史配置样例，在种入记录时冻结，不按当前配置或查询时的结果反推。 */
export function createProbeRecordSlowThresholdsFixture(
  record: MockProbeRecord,
): ProbeRecordSlowThresholds | null {
  const snapshots: Record<string, ProbeRecordSlowThresholds | null> = {
    'probe-001': { firstTokenMs: 30_000, durationMs: 60_000 },
    'probe-008': { firstTokenMs: 1000, durationMs: 30_000 },
    'probe-015': { firstTokenMs: 5000, durationMs: 3000 },
    'probe-019': null,
    'probe-029': { firstTokenMs: 3000, durationMs: 5000 },
    'probe-036': { firstTokenMs: 4000, durationMs: 30_000 },
    'probe-043': { firstTokenMs: 5000, durationMs: 30_000 },
  };
  const snapshot = snapshots[record.id];
  if (snapshot === null) return null;
  return { ...(snapshot ?? { firstTokenMs: 10_000, durationMs: 30_000 }) };
}

/** 初始化时冻结展示样例；请求头模拟后续 JSONL 采集，不表示真实后端已经保存。 */
export function createProbeRecordRequestContentFixture(
  record: MockProbeRecord,
): Omit<ProbeRecordRequestContent, 'bodyBytes'> {
  if (record.id === 'probe-019') return { method: 'POST', headers: null, body: null };
  const models: Record<string, string> = {
    'GPT-6 Astra': 'gpt-6-astra',
    'GPT-6.1 Sol': 'gpt-6.1-sol',
    'GPT-5.6 Sol': 'gpt-5.6-sol',
    'Claude Sonnet': 'claude-sonnet',
    'Claude Opus': 'claude-opus',
  };
  const model = models[record.modelName] ?? record.modelName;
  const anthropic = record.siteId === 'site-3';
  const body = anthropic
    ? { model, max_tokens: 8, messages: [{ role: 'user', content: '只回复 OK' }], stream: true }
    : {
        input: '只回复 OK',
        max_output_tokens: 8,
        model,
        reasoning: { effort: 'low' },
        stream: true,
      };
  return {
    method: 'POST',
    headers:
      record.id === 'probe-020'
        ? null
        : [
            { name: 'content-type', value: 'application/json' },
            { name: 'user-agent', value: 'RelayPulse/2.0.0 (macOS 15.0; arm)' },
            ...(anthropic
              ? [
                  { name: 'x-api-key', value: '[REDACTED]' },
                  { name: 'anthropic-version', value: '2023-06-01' },
                ]
              : [{ name: 'authorization', value: '[REDACTED]' }]),
          ],
    body: JSON.stringify(body),
  };
}

/** 独立的响应元数据样例；不扫描真实日志，不读取当前配置或真实文件路径。 */
export function createProbeRecordResponseFixture(
  record: MockProbeRecord,
): ProbeRecordResponseContent {
  const logFile = {
    path: `/mock/RelayPulse/probe-records/${record.id}/response.jsonl`,
    canOpenDirectory: true,
  };
  if (record.result === 'failure' && record.durationMs === null)
    return {
      state: 'not_received',
      kind: null,
      receivedBytes: null,
      headers: null,
      chunkCount: null,
      chunks: null,
      errorResponse: null,
      logFile,
    };
  if (record.id === 'probe-020')
    return {
      state: 'received',
      kind: 'stream',
      receivedBytes: 3404,
      headers: null,
      chunkCount: null,
      chunks: null,
      errorResponse: null,
      logFile,
    };
  const error = record.result === 'failure' ? failureResponseFixture(record.id) : null;
  const kind = error?.kind ?? 'stream';
  const bytes =
    record.id === 'probe-001'
      ? [22528, 22528, 248, 239, 225, 870, 192, 240, 310, 25979]
      : [1363, 1369, 672];
  const elapsed = [38491, 38736, 38736, 38745, 38745, 38745, 38745, 38745, 38745, 39868];
  const chunks =
    kind === 'stream'
      ? bytes.map((size, index) => ({
          index: index + 1,
          elapsedMs: record.id === 'probe-001' ? elapsed[index] : 100 + index * 90,
          bytes: size,
        }))
      : null;
  const responseChunks = record.id === 'probe-010' ? [] : chunks;
  let receivedBytes: number | null =
    record.id === 'probe-010' ? 0 : (chunks?.reduce((sum, chunk) => sum + chunk.bytes, 0) ?? 159);
  if (error)
    receivedBytes = error.body === null ? null : new TextEncoder().encode(error.body).byteLength;
  const framingHeaders: ProbeRecordResponseHeader[] = [];
  if (kind === 'stream') framingHeaders.push({ name: 'transfer-encoding', value: 'chunked' });
  else if (receivedBytes !== null)
    framingHeaders.push({ name: 'content-length', value: String(receivedBytes) });
  const contentTypes = {
    stream: 'text/event-stream',
    json: 'application/json; charset=utf-8',
    other: 'text/html; charset=utf-8',
  };
  return {
    state: 'received',
    kind,
    receivedBytes,
    headers: [
      { name: 'date', value: new Date(record.probedAt).toUTCString() },
      { name: 'content-type', value: contentTypes[kind] },
      ...framingHeaders,
      { name: 'connection', value: 'keep-alive' },
      { name: 'server', value: 'mock-provider' },
      { name: 'nel', value: '{"report_to":"mock-report","success_fraction":0.0,"max_age":604800}' },
      { name: 'referrer-policy', value: 'strict-origin-when-cross-origin' },
      { name: 'x-client-request-id', value: `mock-client-${record.id}` },
      { name: 'x-content-type-options', value: 'nosniff' },
      { name: 'x-frame-options', value: 'DENY' },
      { name: 'x-request-id', value: `mock-request-${record.id}` },
      { name: 'cache-control', value: 'no-cache' },
      { name: 'cf-cache-status', value: 'DYNAMIC' },
      {
        name: 'report-to',
        value:
          '{"group":"mock-report","max_age":604800,"endpoints":[{"url":"https://telemetry.example.test/report"}]}',
      },
      { name: 'cf-ray', value: `mock-${record.id}-SAMPLE` },
      { name: 'alt-svc', value: 'h3=":443"; ma=86400' },
    ],
    chunkCount: responseChunks === null ? null : responseChunks.length,
    chunks: responseChunks,
    errorResponse: error ? { body: error.body, bodyComplete: error.bodyComplete } : null,
    logFile,
  };
}
