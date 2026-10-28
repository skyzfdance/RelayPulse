/**
 * 探测记录浏览器 Mock：会话内保存正式历史，先筛选/聚合/排序再分页。
 * 单条删除与全部清理只修改本模块，不导入站点状态、配置或其他业务历史。
 */
import type { MockHandler } from '../registry';
import type { MockProbeRecord } from './records-fixtures';
import type {
  OpenProbeRecordLogResult,
  ProbeRecordDetail,
  ProbeRecordListItem,
  ProbeRecordOptions,
  ProbeRecordRequestContent,
  ProbeRecordRequestInfo,
  ProbeRecordResponseContent,
  ProbeRecordSlowCheck,
  ProbeRecordSlowEvaluation,
  ProbeRecordSlowThresholds,
  ProbeRecordsQuery,
  ProbeTokenUsage,
  RecordsPage,
  RecordTimeRange,
  SiteTokenStats,
  TokenStatsPage,
  TokenStatsQuery,
} from '@/types/domain/records';

import { RelayPulseBackendError } from '@/utils/backend-error';

import { registerMockHandler } from '../registry';
import {
  createProbeRecordFixtures,
  createProbeRecordRequestContentFixture,
  createProbeRecordRequestInfoFixture,
  createProbeRecordResponseFixture,
  createProbeRecordSlowThresholdsFixture,
} from './records-fixtures';

const RETENTION_DAYS = 30;
const DAY = 86_400_000;
const clone = <T>(value: T): T => (value === undefined ? value : JSON.parse(JSON.stringify(value)));
export const recordsState: { records: MockProbeRecord[] } = { records: [] };
let catalog: ProbeRecordOptions = { sites: [], models: [] };
let requestInfoById = new Map<string, ProbeRecordRequestInfo>();
let slowThresholdsById = new Map<string, ProbeRecordSlowThresholds | null>();
let requestContentById = new Map<string, ProbeRecordRequestContent>();
let responseContentById = new Map<string, ProbeRecordResponseContent>();
const failedOnce = new Set<string>();

function scenario() {
  const params = new URLSearchParams(window.location.search);
  return params.get('recordsScenario') ?? params.get('scenario');
}

export function seedProbeRecords(records = createProbeRecordFixtures()) {
  recordsState.records = clone(records);
  requestInfoById = new Map(
    recordsState.records.map((record) => [record.id, createProbeRecordRequestInfoFixture(record)]),
  );
  slowThresholdsById = new Map(
    recordsState.records.map((record) => [
      record.id,
      createProbeRecordSlowThresholdsFixture(record),
    ]),
  );
  requestContentById = new Map(
    recordsState.records.map((record) => {
      const content = createProbeRecordRequestContentFixture(record);
      return [
        record.id,
        {
          ...content,
          // 模拟后端读取保存的文本后计数，页面和 composable 不计算请求大小。
          bodyBytes:
            content.body === null ? null : new TextEncoder().encode(content.body).byteLength,
        },
      ];
    }),
  );
  responseContentById = new Map(
    recordsState.records.map((record) => [record.id, createProbeRecordResponseFixture(record)]),
  );
  failedOnce.clear();
  const sites = new Map<string, string>();
  const models = new Map<string, Set<string>>();
  for (const record of [...records].sort(
    (a, b) => Date.parse(b.probedAt) - Date.parse(a.probedAt),
  )) {
    if (!sites.has(record.siteId)) sites.set(record.siteId, record.siteName);
    const siteIds = models.get(record.modelName) ?? new Set<string>();
    siteIds.add(record.siteId);
    models.set(record.modelName, siteIds);
  }
  catalog = {
    sites: [...sites].map(([id, name]) => ({ id, name })),
    models: [...models].map(([name, siteIds]) => ({ name, siteIds: [...siteIds] })),
  };
}
seedProbeRecords(scenario() === 'empty' ? [] : createProbeRecordFixtures());

function validation(message: string, field: string): never {
  throw new RelayPulseBackendError({
    code: 'probe_records_validation',
    category: 'validation',
    message,
    field,
    retryable: false,
  });
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    return validation('探测记录请求参数格式无效', 'query');
  return value as Record<string, unknown>;
}
function nullableString(value: unknown, field: string) {
  if (value !== null && (typeof value !== 'string' || !value.trim() || value.length > 200))
    validation('筛选值必须是非空字符串或 null', field);
}
function query(args: unknown, kind: 'records'): ProbeRecordsQuery;
function query(args: unknown, kind: 'tokens'): TokenStatsQuery;
function query(args: unknown, kind: 'records' | 'tokens') {
  const value = object(object(args).query);
  if (typeof value.range !== 'string' || !['24h', '7d', '30d', 'all'].includes(value.range))
    validation('不支持的探测记录时间范围', 'query.range');
  nullableString(value.siteId, 'query.siteId');
  if (!Number.isSafeInteger(value.page) || Number(value.page) < 1)
    validation('页码必须是从 1 开始的整数', 'query.page');
  if (![10, 20, 50, 100].includes(Number(value.pageSize)) || typeof value.pageSize !== 'number')
    validation('每页条数必须为 10、20、50 或 100', 'query.pageSize');
  if (kind === 'records') {
    nullableString(value.modelName, 'query.modelName');
    if (
      value.result !== null &&
      (typeof value.result !== 'string' || !['success', 'slow', 'failure'].includes(value.result))
    )
      validation('不支持的探测结果筛选', 'query.result');
  }
  return value as unknown as ProbeRecordsQuery | TokenStatsQuery;
}

function retainedRecords(now: number) {
  return recordsState.records.filter((record) => {
    const time = Date.parse(record.probedAt);
    return time >= now - RETENTION_DAYS * DAY && time <= now;
  });
}
function filterRecords(
  records: MockProbeRecord[],
  range: RecordTimeRange,
  siteId: string | null,
  now: number,
) {
  if (scenario() === 'no-results') return [];
  const days: Record<RecordTimeRange, number> = {
    '24h': 1,
    '7d': 7,
    '30d': 30,
    all: RETENTION_DAYS,
  };
  return records.filter(
    (record) =>
      Date.parse(record.probedAt) >= now - days[range] * DAY &&
      (!siteId || record.siteId === siteId),
  );
}
/** 统计固定按总 Token 降序，缺失值置后；相等时使用稳定站点 ID 次序。 */
function compareTotals(a: number | null, b: number | null) {
  if (a === null) return b === null ? 0 : 1;
  if (b === null) return -1;
  return b - a;
}
function paginate<T>(
  items: T[],
  input: { page: number; pageSize: number },
  retainedCount: number,
): RecordsPage<T> {
  const page = Math.min(input.page, Math.max(1, Math.ceil(items.length / input.pageSize)));
  return {
    items: items.slice((page - 1) * input.pageSize, page * input.pageSize),
    total: items.length,
    page,
    retainedCount,
  };
}
function hasUsage(record: MockProbeRecord) {
  return record.usage !== null && Object.values(record.usage).some((value) => value !== null);
}
/** 模拟后端解释已保存结果，只使用该条记录及执行时冻结的阈值。 */
function evaluateSlowRecord(record: MockProbeRecord): ProbeRecordSlowEvaluation | null {
  const thresholds = slowThresholdsById.get(record.id);
  if (!thresholds) return null;
  const skipped = record.result === 'failure';
  function check(actual: number | null, threshold: number | null): ProbeRecordSlowCheck {
    if (skipped || actual === null || threshold === null)
      return { comparison: null, triggered: false };
    const comparison = actual >= threshold ? 'gte' : 'lt';
    return { comparison, triggered: record.result === 'slow' && comparison === 'gte' };
  }
  return {
    status: skipped ? 'skipped_failure' : 'evaluated',
    firstToken: check(record.firstTokenMs, thresholds.firstTokenMs),
    duration: check(record.durationMs, thresholds.durationMs),
  };
}
function sumUsage(records: MockProbeRecord[], field: keyof ProbeTokenUsage): number | null {
  const values = records.flatMap((record) => record.usage?.[field] ?? []);
  return values.length ? values.reduce((sum, value) => sum + value, 0) : null;
}

function register(command: string, handler: MockHandler, write = false) {
  registerMockHandler(command, async (args) => {
    const input = args === undefined ? undefined : clone(args);
    const configured = new URLSearchParams(window.location.search).get('recordsDelay');
    let delay = import.meta.env.MODE === 'test' ? 0 : 450;
    if (scenario() === 'slow') delay = 2200;
    if (configured !== null && Number.isFinite(Number(configured)))
      delay = Math.max(0, Math.min(10_000, Number(configured)));
    if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
    const unavailable =
      scenario() === 'error' ||
      (write && scenario() === 'action-error') ||
      (command === 'get_probe_records' && scenario() === 'records-error') ||
      (command === 'get_probe_record_detail' && scenario() === 'detail-error') ||
      (command === 'get_probe_record_request' && scenario() === 'request-error') ||
      (command === 'get_probe_record_response' && scenario() === 'response-error') ||
      (command === 'open_probe_record_log_directory' && scenario() === 'log-open-error') ||
      (command === 'get_probe_token_stats' && scenario() === 'tokens-error') ||
      (command === 'get_probe_record_options' && scenario() === 'options-error');
    const firstFailure = !write && scenario() === 'retry-once' && !failedOnce.has(command);
    if (unavailable || firstFailure) {
      failedOnce.add(command);
      let message = write ? '暂时无法删除探测记录，请重试' : '暂时无法读取探测记录，请重试';
      if (command === 'get_probe_record_response') message = '暂时无法读取原始返回，请重试';
      if (command === 'open_probe_record_log_directory')
        message = '暂时无法打开探测日志目录，请重试';
      throw new RelayPulseBackendError({
        code:
          command === 'open_probe_record_log_directory'
            ? 'probe_record_log_open_failed'
            : 'probe_records_unavailable',
        category: 'unavailable',
        message,
        retryable: true,
      });
    }
    return clone(await handler(input));
  });
}

register('get_probe_record_options', () => catalog);
function requireRecord(args: unknown) {
  const recordId = object(args).recordId;
  if (typeof recordId !== 'string' || !recordId.trim()) validation('记录 ID 不能为空', 'recordId');
  const record = retainedRecords(Date.now()).find((item) => item.id === recordId);
  if (!record || scenario() === 'detail-missing')
    throw new RelayPulseBackendError({
      code: 'probe_record_not_found',
      category: 'not_found',
      message: '这条探测记录已不存在或已过期',
      retryable: false,
    });
  return record;
}
register('get_probe_record_detail', (args): ProbeRecordDetail => {
  const record = requireRecord(args);
  const recordId = record.id;
  const request = requestContentById.get(recordId);
  const response = responseContentById.get(recordId);
  return {
    id: record.id,
    siteName: record.siteName,
    modelName: record.modelName,
    probedAt: record.probedAt,
    result: record.result,
    resultMessage: record.resultMessage,
    firstTokenMs: record.firstTokenMs,
    durationMs: record.durationMs,
    usage: record.usage,
    usageSource: hasUsage(record) ? 'provider' : 'unreported',
    requestInfo: requestInfoById.get(recordId) ?? {
      endpoint: null,
      strategyLabel: null,
      httpStatus: null,
      requestId: null,
    },
    slowThresholds: slowThresholdsById.get(recordId) ?? null,
    slowEvaluation: evaluateSlowRecord(record),
    requestSummary: { method: request?.method ?? null, bodyBytes: request?.bodyBytes ?? null },
    responseSummary: {
      state: response?.state ?? 'unknown',
      kind: response?.kind ?? null,
      receivedBytes: response?.receivedBytes ?? null,
    },
  };
});
register('get_probe_record_request', (args): ProbeRecordRequestContent => {
  const record = requireRecord(args);
  if (scenario() === 'request-missing')
    throw new RelayPulseBackendError({
      code: 'probe_record_request_unavailable',
      category: 'unavailable',
      message: '未找到这条记录的请求内容文件',
      retryable: false,
    });
  return (
    requestContentById.get(record.id) ?? {
      method: null,
      bodyBytes: null,
      headers: null,
      body: null,
    }
  );
});
register('get_probe_record_response', (args): ProbeRecordResponseContent => {
  const record = requireRecord(args);
  const response = responseContentById.get(record.id);
  if (!response || scenario() === 'response-missing')
    throw new RelayPulseBackendError({
      code: 'probe_record_response_unavailable',
      category: 'unavailable',
      message: '未找到这条记录的响应日志文件',
      retryable: false,
    });
  return response;
});
register(
  'open_probe_record_log_directory',
  (args): OpenProbeRecordLogResult => {
    const record = requireRecord(args);
    const logFile = responseContentById.get(record.id)?.logFile;
    if (!logFile?.canOpenDirectory || scenario() === 'response-missing')
      throw new RelayPulseBackendError({
        code: 'probe_record_log_unavailable',
        category: 'unavailable',
        message: '这条记录的日志目录不可用',
        retryable: false,
      });
    return { message: '浏览器预览：已模拟打开日志目录' };
  },
  true,
);
register('get_probe_records', (args) => {
  const input = query(args, 'records');
  const now = Date.now();
  const retained = retainedRecords(now);
  const filtered = filterRecords(retained, input.range, input.siteId, now).filter(
    (record) =>
      (!input.modelName || record.modelName === input.modelName) &&
      (!input.result || record.result === input.result),
  );
  filtered.sort(
    (a, b) => Date.parse(b.probedAt) - Date.parse(a.probedAt) || a.id.localeCompare(b.id),
  );
  const items: ProbeRecordListItem[] = filtered.map((record) => ({
    id: record.id,
    siteName: record.siteName,
    modelName: record.modelName,
    probedAt: record.probedAt,
    result: record.result,
    resultMessage: record.resultMessage,
    firstTokenMs: record.firstTokenMs,
    durationMs: record.durationMs,
    totalTokens: record.usage?.totalTokens ?? null,
  }));
  return paginate(items, input, retained.length);
});
register('get_probe_token_stats', (args): TokenStatsPage => {
  const input = query(args, 'tokens');
  const now = Date.now();
  const retained = retainedRecords(now);
  const records = filterRecords(retained, input.range, input.siteId, now);
  const groups = new Map<string, MockProbeRecord[]>();
  for (const record of records) {
    const group = groups.get(record.siteId) ?? [];
    group.push(record);
    groups.set(record.siteId, group);
  }
  const items: SiteTokenStats[] = [...groups].map(([siteId, group]) => {
    const latest = [...group].sort(
      (a, b) => Date.parse(b.probedAt) - Date.parse(a.probedAt) || a.id.localeCompare(b.id),
    )[0];
    const totalTokens = sumUsage(group, 'totalTokens');
    const totalReportedCount = group.filter((record) => record.usage?.totalTokens != null).length;
    return {
      siteId,
      siteName: latest.siteName,
      probeCount: group.length,
      reportedCount: group.filter(hasUsage).length,
      totalReportedCount,
      inputTokens: sumUsage(group, 'inputTokens'),
      outputTokens: sumUsage(group, 'outputTokens'),
      totalTokens,
      averageTokens: totalTokens === null ? null : totalTokens / totalReportedCount,
    };
  });
  items.sort(
    (a, b) => compareTotals(a.totalTokens, b.totalTokens) || a.siteId.localeCompare(b.siteId),
  );
  const reportedCount = records.filter(hasUsage).length;
  const totalReportedCount = records.filter((record) => record.usage?.totalTokens != null).length;
  const totalTokens = sumUsage(records, 'totalTokens');
  return {
    ...paginate(items, input, retained.length),
    summary: {
      probeCount: records.length,
      reportedCount,
      totalReportedCount,
      inputTokens: sumUsage(records, 'inputTokens'),
      outputTokens: sumUsage(records, 'outputTokens'),
      totalTokens,
      averageTokens: totalTokens === null ? null : totalTokens / totalReportedCount,
    },
  };
});
register(
  'delete_probe_record',
  (args) => {
    const recordId = object(args).recordId;
    if (typeof recordId !== 'string' || !recordId.trim())
      validation('记录 ID 不能为空', 'recordId');
    const index = recordsState.records.findIndex((record) => record.id === recordId);
    if (index < 0)
      throw new RelayPulseBackendError({
        code: 'probe_record_not_found',
        category: 'not_found',
        message: '这条探测记录已不存在，请刷新列表',
        retryable: false,
      });
    recordsState.records.splice(index, 1);
    requestInfoById.delete(recordId);
    slowThresholdsById.delete(recordId);
    requestContentById.delete(recordId);
    responseContentById.delete(recordId);
  },
  true,
);
register(
  'clear_probe_records',
  () => {
    recordsState.records = [];
    requestInfoById.clear();
    slowThresholdsById.clear();
    requestContentById.clear();
    responseContentById.clear();
  },
  true,
);
