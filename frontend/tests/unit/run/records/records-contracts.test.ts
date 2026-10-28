import type { MockProbeRecord } from '@/mocks/handlers/run/records-fixtures';
import type { ProbeRecordsQuery, TokenStatsQuery } from '@/types/domain/records';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';

import {
  clearProbeRecords,
  deleteProbeRecord,
  fetchProbeRecordDetail,
  fetchProbeRecordOptions,
  fetchProbeRecordRequest,
  fetchProbeRecordResponse,
  fetchProbeRecords,
  fetchProbeTokenStats,
  openProbeRecordLogDirectory,
} from '@/api/run/records';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state as sitesState } from '@/mocks/handlers/config/sites';
import { recordsState, seedProbeRecords } from '@/mocks/handlers/run/records';
import { createProbeRecordFixtures } from '@/mocks/handlers/run/records-fixtures';

import { setPolicyPrompt } from '../../../support/parameter-fields';

const recordQuery = (patch: Partial<ProbeRecordsQuery> = {}): ProbeRecordsQuery => ({
  range: '24h',
  siteId: null,
  modelName: null,
  result: null,
  page: 1,
  pageSize: 10,
  ...patch,
});
const tokenQuery = (patch: Partial<TokenStatsQuery> = {}): TokenStatsQuery => ({
  range: '24h',
  siteId: null,
  page: 1,
  pageSize: 10,
  ...patch,
});
function record(id: string, patch: Partial<MockProbeRecord> = {}): MockProbeRecord {
  return { ...createProbeRecordFixtures()[0], id, ...patch };
}

beforeEach(() => {
  window.history.replaceState({}, '', '/run/records');
  seedProbeRecords();
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  vi.useRealTimers();
  window.history.replaceState({}, '', '/');
  resetFeedbackForTests();
});

describe('探测记录的前端命令契约和 Mock', () => {
  it('命令只返回当前页面需要的字段；列表摘要与详情完整用量分别读取', async () => {
    const keys = (value: object) => Object.keys(value).sort();
    const options = await fetchProbeRecordOptions();
    expect(keys(options)).toEqual(['models', 'sites']);
    const list = await fetchProbeRecords(recordQuery());
    expect(keys(list)).toEqual(['items', 'page', 'retainedCount', 'total']);
    expect(keys(list.items[0])).toEqual([
      'durationMs',
      'firstTokenMs',
      'id',
      'modelName',
      'probedAt',
      'result',
      'resultMessage',
      'siteName',
      'totalTokens',
    ]);
    expect(list.items[0].totalTokens).toBe(4394);
    const detail = await fetchProbeRecordDetail(list.items[0].id);
    expect(keys(detail)).toEqual([
      'durationMs',
      'firstTokenMs',
      'id',
      'modelName',
      'probedAt',
      'requestInfo',
      'requestSummary',
      'responseSummary',
      'result',
      'resultMessage',
      'siteName',
      'slowEvaluation',
      'slowThresholds',
      'usage',
      'usageSource',
    ]);
    expect(detail.usage).toEqual({ inputTokens: 4389, outputTokens: 5, totalTokens: 4394 });
    const stats = await fetchProbeTokenStats(tokenQuery());
    expect(keys(stats)).toEqual(['items', 'page', 'retainedCount', 'summary', 'total']);
    expect(keys(stats.summary)).toEqual([
      'averageTokens',
      'inputTokens',
      'outputTokens',
      'probeCount',
      'reportedCount',
      'totalReportedCount',
      'totalTokens',
    ]);
  });

  it('响应元数据和摘要按记录返回，保留超过 64 KiB 的真实大小，不预带合并正文', async () => {
    const detail = await fetchProbeRecordDetail('probe-001');
    const response = await fetchProbeRecordResponse(detail.id);
    expect(detail.responseSummary).toEqual({
      state: response.state,
      kind: response.kind,
      receivedBytes: response.receivedBytes,
    });
    expect(response.receivedBytes).toBeGreaterThan(64 * 1024);
    expect(response.headers).toContainEqual({ name: 'content-type', value: 'text/event-stream' });
    expect(response.chunkCount).toBe(response.chunks!.length);
    expect(detail).not.toHaveProperty('chunks');
    expect(detail).not.toHaveProperty('logFile');
    expect(response).not.toHaveProperty('body');
    expect(response.errorResponse).toBeNull();
    const snapshot = JSON.parse(JSON.stringify(response));
    response.chunks![0].bytes = 0;
    response.logFile!.path = '未保存的路径';
    response.headers![0].value = '未保存的响应头';
    setPolicyPrompt(sitesState.sites[0].policy, '后续修改的提示词');
    expect(await fetchProbeRecordResponse(detail.id)).toEqual(snapshot);
    expect(await fetchProbeRecordResponse('probe-020')).toMatchObject({
      state: 'received',
      headers: null,
      chunks: null,
      chunkCount: null,
    });
    expect(await fetchProbeRecordResponse('probe-012')).toMatchObject({
      state: 'not_received',
      kind: null,
      receivedBytes: null,
      errorResponse: null,
    });
    expect(await fetchProbeRecordResponse('probe-010')).toMatchObject({
      receivedBytes: 0,
      chunkCount: 0,
      chunks: [],
    });
    expect(await fetchProbeRecordResponse('probe-002')).toMatchObject({
      state: 'received',
      kind: 'json',
    });
  });

  it('错误响应体依据单次失败结果返回，覆盖 HTTP 自定义码及 200 的业务失败，不改写历史', async () => {
    for (const [id, status] of [
      ['probe-002', 503],
      ['probe-003', 529],
      ['probe-004', 400],
      ['probe-007', 200],
      ['probe-032', 302],
    ] as const) {
      const detail = await fetchProbeRecordDetail(id);
      const response = await fetchProbeRecordResponse(id);
      expect(detail.result).toBe('failure');
      expect(detail.requestInfo.httpStatus).toBe(status);
      expect(response.state).toBe('received');
      expect(response.errorResponse).toEqual({ body: expect.any(String), bodyComplete: true });
      expect(response.receivedBytes).toBe(
        new TextEncoder().encode(response.errorResponse!.body!).byteLength,
      );
      expect(detail).not.toHaveProperty('errorResponse');
      const snapshot = JSON.parse(JSON.stringify(response));
      response.errorResponse!.body = '未保存的内容';
      expect(await fetchProbeRecordResponse(id)).toEqual(snapshot);
    }
    for (const id of ['probe-001', 'probe-005', 'probe-012'])
      expect((await fetchProbeRecordResponse(id)).errorResponse).toBeNull();
    expect((await fetchProbeRecordResponse('probe-017')).errorResponse).toEqual({
      body: '',
      bodyComplete: true,
    });
    expect((await fetchProbeRecordResponse('probe-022')).errorResponse).toEqual({
      body: null,
      bodyComplete: null,
    });
    expect((await fetchProbeRecordResponse('probe-027')).errorResponse?.bodyComplete).toBe(false);
  });

  it('响应读取和打开日志遵守错误及删除边界，模拟打开不修改业务状态', async () => {
    const before = JSON.stringify({ recordsState, sitesState });
    expect(await openProbeRecordLogDirectory('probe-001')).toEqual({
      message: '浏览器预览：已模拟打开日志目录',
    });
    expect(JSON.stringify({ recordsState, sitesState })).toBe(before);
    await expect(openProbeRecordLogDirectory(' ')).rejects.toMatchObject({
      code: 'probe_records_validation',
      field: 'recordId',
    });
    await expect(fetchProbeRecordResponse(' ')).rejects.toMatchObject({
      code: 'probe_records_validation',
      field: 'recordId',
    });
    window.history.replaceState({}, '', '/run/records?recordsScenario=response-missing');
    await expect(fetchProbeRecordResponse('probe-001')).rejects.toMatchObject({
      code: 'probe_record_response_unavailable',
      retryable: false,
    });
    await expect(openProbeRecordLogDirectory('probe-001')).rejects.toMatchObject({
      code: 'probe_record_log_unavailable',
    });
    window.history.replaceState({}, '', '/run/records?recordsScenario=log-open-error');
    await expect(openProbeRecordLogDirectory('probe-001')).rejects.toMatchObject({
      code: 'probe_record_log_open_failed',
      retryable: true,
    });
    window.history.replaceState({}, '', '/run/records?recordsScenario=response-error');
    await expect(fetchProbeRecordResponse('probe-001')).rejects.toMatchObject({
      code: 'probe_records_unavailable',
      retryable: true,
    });
    window.history.replaceState({}, '', '/run/records?recordsScenario=retry-once');
    seedProbeRecords();
    await expect(fetchProbeRecordResponse('probe-001')).rejects.toThrow();
    expect((await fetchProbeRecordResponse('probe-001')).kind).toBe('stream');
    window.history.replaceState({}, '', '/run/records');
    await deleteProbeRecord('probe-001');
    await expect(fetchProbeRecordResponse('probe-001')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
    await expect(openProbeRecordLogDirectory('probe-001')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
    await clearProbeRecords();
    await expect(fetchProbeRecordResponse('probe-002')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
  });

  it('请求内容按记录读取；大小由 mock 后端计算 UTF-8 字节数，摘要不预带请求内容', async () => {
    const detail = await fetchProbeRecordDetail('probe-001');
    const request = await fetchProbeRecordRequest(detail.id);
    expect(detail.requestSummary).toEqual({ method: 'POST', bodyBytes: request.bodyBytes });
    expect(detail).not.toHaveProperty('headers');
    expect(detail).not.toHaveProperty('body');
    expect(request.headers).toContainEqual({ name: 'authorization', value: '[REDACTED]' });
    expect(request.body).toContain('只回复 OK');
    expect(request.bodyBytes).toBe(new TextEncoder().encode(request.body!).byteLength);
    expect(request.bodyBytes).toBeGreaterThan(request.body!.length);
    const snapshot = JSON.parse(JSON.stringify(request));
    request.body = '未保存的请求体';
    request.headers![0].value = '未保存的请求头';
    setPolicyPrompt(sitesState.sites[0].policy, '后续修改的提示词');
    expect(await fetchProbeRecordRequest(detail.id)).toEqual(snapshot);
    expect((await fetchProbeRecordDetail(detail.id)).requestSummary).toEqual(detail.requestSummary);
    expect(await fetchProbeRecordRequest('probe-019')).toMatchObject({
      headers: null,
      bodyBytes: null,
      body: null,
    });
    expect(await fetchProbeRecordRequest('probe-020')).toMatchObject({
      headers: null,
      body: expect.any(String),
      bodyBytes: expect.any(Number),
    });
  });

  it('请求内容读取区分文件不可用、临时错误、无效 ID，以及已删除或过期的记录', async () => {
    window.history.replaceState({}, '', '/run/records?recordsScenario=request-error');
    await expect(fetchProbeRecordRequest('probe-001')).rejects.toMatchObject({
      code: 'probe_records_unavailable',
      retryable: true,
    });
    expect((await fetchProbeRecordDetail('probe-001')).id).toBe('probe-001');
    window.history.replaceState({}, '', '/run/records?recordsScenario=request-missing');
    await expect(fetchProbeRecordRequest('probe-001')).rejects.toMatchObject({
      code: 'probe_record_request_unavailable',
      retryable: false,
    });
    window.history.replaceState({}, '', '/run/records?recordsScenario=retry-once');
    seedProbeRecords();
    await expect(fetchProbeRecordRequest('probe-001')).rejects.toThrow();
    expect((await fetchProbeRecordRequest('probe-001')).body).toContain('只回复 OK');
    window.history.replaceState({}, '', '/run/records');
    await expect(fetchProbeRecordRequest(' ')).rejects.toMatchObject({
      code: 'probe_records_validation',
      field: 'recordId',
    });
    await expect(fetchProbeRecordRequest('not-found')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
    seedProbeRecords([
      record('expired-request', { probedAt: new Date(Date.now() - 31 * 86_400_000).toISOString() }),
    ]);
    await expect(fetchProbeRecordRequest('expired-request')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
    seedProbeRecords();
    await deleteProbeRecord('probe-001');
    await expect(fetchProbeRecordRequest('probe-001')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
    await clearProbeRecords();
    await expect(fetchProbeRecordRequest('probe-002')).rejects.toMatchObject({
      code: 'probe_record_not_found',
    });
  });

  it('详情返回独立的请求快照，不把正文或当前健康状态混入详情上半部分', async () => {
    const beforeSites = JSON.stringify(sitesState);
    const beforeRecords = JSON.stringify(recordsState);
    const detail = await fetchProbeRecordDetail('probe-001');
    expect(detail).toMatchObject({
      id: 'probe-001',
      siteName: 'ai.input.im',
      modelName: 'GPT-6 Astra',
      result: 'slow',
      resultMessage: '响应偏慢',
      firstTokenMs: 38_740,
      durationMs: 39_870,
      usage: { inputTokens: 4389, outputTokens: 5, totalTokens: 4394 },
      probedAt: expect.any(String),
      requestInfo: {
        endpoint: 'https://input.codes/v1/responses',
        strategyLabel: '低推理',
        httpStatus: 200,
        requestId: 'mock-request-probe-001',
      },
      usageSource: 'provider',
      slowThresholds: { firstTokenMs: 30_000, durationMs: 60_000 },
      slowEvaluation: {
        status: 'evaluated',
        firstToken: { comparison: 'gte', triggered: true },
        duration: { comparison: 'lt', triggered: false },
      },
    });
    expect(detail).not.toHaveProperty('health');
    expect(detail).not.toHaveProperty('requestBody');
    expect(detail).not.toHaveProperty('responseBody');
    detail.requestInfo.endpoint = '未保存的端点';
    detail.slowThresholds!.firstTokenMs = 1;
    detail.slowEvaluation!.firstToken.comparison = 'lt';
    expect((await fetchProbeRecordDetail('probe-001')).requestInfo.endpoint).toBe(
      'https://input.codes/v1/responses',
    );
    expect((await fetchProbeRecordDetail('probe-001')).slowThresholds).toEqual({
      firstTokenMs: 30_000,
      durationMs: 60_000,
    });
    expect((await fetchProbeRecordDetail('probe-001')).slowEvaluation!.firstToken).toEqual({
      comparison: 'gte',
      triggered: true,
    });
    expect(JSON.stringify(sitesState)).toBe(beforeSites);
    expect(JSON.stringify(recordsState)).toBe(beforeRecords);
  });

  it('当前站点设置变化不改写历史阈值或历史结果，快照缺失时不从当前配置补全', async () => {
    const before = await fetchProbeRecordDetail('probe-001');
    sitesState.sites[0].connection.slowFirstTokenSeconds = 90;
    sitesState.sites[0].connection.slowTotalSeconds = 120;
    expect(await fetchProbeRecordDetail('probe-001')).toEqual(before);
    expect(await fetchProbeRecordDetail('probe-019')).toMatchObject({
      slowThresholds: null,
      slowEvaluation: null,
    });
  });

  it('正常 mock 的偏慢结果与冻结阈值一致，同时覆盖首 Token、总耗时和两项触发', async () => {
    const firstOnly = await fetchProbeRecordDetail('probe-001');
    expect(firstOnly.firstTokenMs).toBeGreaterThanOrEqual(firstOnly.slowThresholds!.firstTokenMs!);
    expect(firstOnly.durationMs).toBeLessThan(firstOnly.slowThresholds!.durationMs!);
    const totalOnly = await fetchProbeRecordDetail('probe-015');
    expect(totalOnly.firstTokenMs).toBeLessThan(totalOnly.slowThresholds!.firstTokenMs!);
    expect(totalOnly.durationMs).toBeGreaterThanOrEqual(totalOnly.slowThresholds!.durationMs!);
    expect(totalOnly.slowEvaluation).toEqual({
      status: 'evaluated',
      firstToken: { comparison: 'lt', triggered: false },
      duration: { comparison: 'gte', triggered: true },
    });
    const both = await fetchProbeRecordDetail('probe-029');
    expect(both.firstTokenMs).toBeGreaterThanOrEqual(both.slowThresholds!.firstTokenMs!);
    expect(both.durationMs).toBeGreaterThanOrEqual(both.slowThresholds!.durationMs!);
    expect(both.slowEvaluation).toEqual({
      status: 'evaluated',
      firstToken: { comparison: 'gte', triggered: true },
      duration: { comparison: 'gte', triggered: true },
    });
    for (const record of recordsState.records.filter((item) => item.result !== 'failure')) {
      const detail = await fetchProbeRecordDetail(record.id);
      if (!detail.slowThresholds) continue;
      const reached =
        (detail.firstTokenMs !== null &&
          detail.firstTokenMs >= detail.slowThresholds.firstTokenMs!) ||
        (detail.durationMs !== null && detail.durationMs >= detail.slowThresholds.durationMs!);
      expect(detail.result, detail.id).toBe(reached ? 'slow' : 'success');
    }
  });

  it.each([
    [10_000, 29_999, 'slow', 'gte', 'lt'],
    [9999, 30_000, 'slow', 'lt', 'gte'],
    [null, 30_000, 'slow', null, 'gte'],
    [null, null, 'success', null, null],
    [10_000, 30_000, 'failure', null, null],
  ] as const)(
    'mock 后端生成判定依据：首 Token %s，总耗时 %s，结果 %s',
    async (firstTokenMs, durationMs, result, firstComparison, totalComparison) => {
      seedProbeRecords([record('criteria-boundary', { firstTokenMs, durationMs, result })]);
      const detail = await fetchProbeRecordDetail('criteria-boundary');
      expect(detail.slowThresholds).toEqual({ firstTokenMs: 10_000, durationMs: 30_000 });
      expect(detail.slowEvaluation).toEqual({
        status: result === 'failure' ? 'skipped_failure' : 'evaluated',
        firstToken: {
          comparison: firstComparison,
          triggered: result === 'slow' && firstComparison === 'gte',
        },
        duration: {
          comparison: totalComparison,
          triggered: result === 'slow' && totalComparison === 'gte',
        },
      });
    },
  );

  it('HTTP 失败与没有取得 HTTP 响应分开；缺失数据与明确零值保持原样', async () => {
    expect(await fetchProbeRecordDetail('probe-002')).toMatchObject({
      result: 'failure',
      firstTokenMs: null,
      usage: null,
      usageSource: 'unreported',
      requestInfo: { httpStatus: 503, requestId: 'mock-request-probe-002' },
      slowEvaluation: {
        status: 'skipped_failure',
        firstToken: { comparison: null, triggered: false },
        duration: { comparison: null, triggered: false },
      },
    });
    expect(await fetchProbeRecordDetail('probe-012')).toMatchObject({
      firstTokenMs: null,
      durationMs: null,
      usage: null,
      usageSource: 'unreported',
      requestInfo: { httpStatus: null, requestId: null },
    });
    expect((await fetchProbeRecordDetail('probe-010')).usage).toEqual({
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
    });
    expect((await fetchProbeRecordDetail('probe-010')).usageSource).toBe('provider');
    expect((await fetchProbeRecordDetail('probe-009')).usageSource).toBe('provider');
  });

  it('不存在、过期、删除或清理后的记录不能继续读取详情', async () => {
    const expectMissing = (id: string) =>
      expect(fetchProbeRecordDetail(id)).rejects.toMatchObject({
        code: 'probe_record_not_found',
        retryable: false,
      });
    await expectMissing('not-found');
    seedProbeRecords([
      record('expired-detail', { probedAt: new Date(Date.now() - 31 * 86_400_000).toISOString() }),
    ]);
    await expectMissing('expired-detail');
    seedProbeRecords();
    await deleteProbeRecord('probe-001');
    await expectMissing('probe-001');
    await clearProbeRecords();
    await expectMissing('probe-002');
  });

  it('详情读取失败保持错误语义，空 ID 返回校验错误', async () => {
    window.history.replaceState({}, '', '/run/records?recordsScenario=detail-error');
    await expect(fetchProbeRecordDetail('probe-001')).rejects.toMatchObject({
      code: 'probe_records_unavailable',
      retryable: true,
    });
    expect((await fetchProbeRecords(recordQuery())).total).toBe(30);
    window.history.replaceState({}, '', '/run/records?recordsScenario=detail-missing');
    await expect(fetchProbeRecordDetail('probe-001')).rejects.toMatchObject({
      code: 'probe_record_not_found',
      retryable: false,
    });
    window.history.replaceState({}, '', '/run/records');
    await expect(fetchProbeRecordDetail(' ')).rejects.toMatchObject({
      code: 'probe_records_validation',
      field: 'recordId',
    });
  });

  it('默认时间倒序，先对完整集合筛选再分页，返回的是独立摘要', async () => {
    const first = await fetchProbeRecords(reactive(recordQuery()));
    const second = await fetchProbeRecords(recordQuery({ page: 2 }));
    expect(first).toMatchObject({ total: 30, retainedCount: 48, page: 1 });
    expect(first.items.map((item) => item.id).slice(0, 4)).toEqual([
      'probe-001',
      'probe-002',
      'probe-003',
      'probe-004',
    ]);
    expect(new Set([...first.items, ...second.items].map((item) => item.id)).size).toBe(20);
    expect(first.items[0]).not.toHaveProperty('health');
    first.items[0].siteName = '未保存的名称';
    first.items[0].totalTokens = 99;
    expect((await fetchProbeRecords(recordQuery())).items[0]).toMatchObject({
      siteName: 'ai.input.im',
      totalTokens: 4394,
    });
    const options = await fetchProbeRecordOptions();
    options.sites[0].name = '未保存的选项';
    expect((await fetchProbeRecordOptions()).sites[0].name).not.toBe('未保存的选项');
  });

  it('时间、站点、模型、单次结果共同筛选；没有匹配时保留全局记录数', async () => {
    const filtered = await fetchProbeRecords(
      recordQuery({
        range: 'all',
        siteId: 'site-2',
        modelName: 'GPT-6 Astra',
        result: 'failure',
      }),
    );
    expect(filtered.items.length).toBeGreaterThan(0);
    expect(
      filtered.items.every(
        (item) =>
          item.siteName === 'doudui.cc' &&
          item.modelName === 'GPT-6 Astra' &&
          item.result === 'failure',
      ),
    ).toBe(true);
    const missing = await fetchProbeRecords(recordQuery({ modelName: '没有这个模型' }));
    expect(missing).toMatchObject({ items: [], total: 0, page: 1, retainedCount: 48 });
  });

  it('记录固定时间倒序，同一时间按记录 ID 稳定排序，分页不会改变顺序', async () => {
    const all = await fetchProbeRecords(recordQuery({ range: 'all', pageSize: 100 }));
    const times = all.items.map((item) => Date.parse(item.probedAt));
    expect(times).toEqual([...times].sort((a, b) => b - a));
    const second = await fetchProbeRecords(recordQuery({ range: 'all', page: 2 }));
    expect(second.items.map((item) => item.id)).toEqual(
      all.items.slice(10, 20).map((item) => item.id),
    );
    const sameTime = new Date(Date.now() - 60_000).toISOString();
    seedProbeRecords([
      record('same-b', { probedAt: sameTime }),
      record('same-a', { probedAt: sameTime }),
    ]);
    expect((await fetchProbeRecords(recordQuery())).items.map((item) => item.id)).toEqual([
      'same-a',
      'same-b',
    ]);
  });

  it('时间窗口含起止边界，过期记录和未来记录均不进入列表或统计', async () => {
    vi.useFakeTimers();
    const now = new Date('2026-10-23T06:00:00Z').getTime();
    vi.setSystemTime(now);
    seedProbeRecords([
      record('at-start', { probedAt: new Date(now - 86_400_000).toISOString() }),
      record('at-end', { probedAt: new Date(now).toISOString() }),
      record('older', { probedAt: new Date(now - 86_400_001).toISOString() }),
      record('expired', { probedAt: new Date(now - 31 * 86_400_000).toISOString() }),
      record('future', { probedAt: new Date(now + 1).toISOString() }),
    ]);
    expect((await fetchProbeRecords(recordQuery())).items.map((item) => item.id)).toEqual([
      'at-end',
      'at-start',
    ]);
    expect((await fetchProbeRecords(recordQuery({ range: 'all' }))).total).toBe(3);
    expect((await fetchProbeTokenStats(tokenQuery())).summary.probeCount).toBe(2);
  });

  it('统计区分未上报、部分上报和明确的 0；平均值只用已报告总量的请求作分母', async () => {
    seedProbeRecords([
      record('none', { usage: null }),
      record('zero', { usage: { inputTokens: 0, outputTokens: 0, totalTokens: 0 } }),
      record('partial', { usage: { inputTokens: 7, outputTokens: null, totalTokens: null } }),
      record('reported', { usage: { inputTokens: 12, outputTokens: 8, totalTokens: 20 } }),
      record('missing-site', { siteId: 'site-2', siteName: '无用量站点', usage: null }),
    ]);
    const stats = await fetchProbeTokenStats(tokenQuery());
    expect(stats.summary).toEqual({
      probeCount: 5,
      reportedCount: 3,
      totalReportedCount: 2,
      inputTokens: 19,
      outputTokens: 8,
      totalTokens: 20,
      averageTokens: 10,
    });
    expect(stats.items[0]).toMatchObject({
      siteId: 'site-1',
      probeCount: 4,
      reportedCount: 3,
      totalReportedCount: 2,
      inputTokens: 19,
      outputTokens: 8,
      totalTokens: 20,
      averageTokens: 10,
    });
    expect(stats.items[1]).toMatchObject({
      reportedCount: 0,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      averageTokens: null,
    });
    const partial = await fetchProbeRecords(recordQuery({ pageSize: 100 }));
    expect(partial.items.find((item) => item.id === 'partial')?.totalTokens).toBeNull();
    expect(partial.items.find((item) => item.id === 'zero')?.totalTokens).toBe(0);
  });

  it('按站点 ID 聚合，同名站点不合并；名称使用匹配窗口内的最新快照', async () => {
    seedProbeRecords([
      record('old', {
        siteName: '旧名称',
        probedAt: new Date(Date.now() - 3600_000).toISOString(),
      }),
      record('new', { siteName: '同名站点' }),
      record('other', { siteId: 'other-site', siteName: '同名站点' }),
    ]);
    const stats = await fetchProbeTokenStats(tokenQuery());
    expect(stats.items).toHaveLength(2);
    expect(stats.items.find((site) => site.siteId === 'site-1')).toMatchObject({
      siteName: '同名站点',
      probeCount: 2,
    });
    expect(
      (await fetchProbeTokenStats(tokenQuery({ siteId: 'other-site' }))).summary.probeCount,
    ).toBe(1);
  });

  it('Token 列表先聚合排序再分页，表内合计不受分页影响', async () => {
    seedProbeRecords(
      Array.from({ length: 14 }, (_, index) =>
        record(`many-${index}`, {
          siteId: `stats-site-${index}`,
          siteName: `站点 ${index}`,
          usage: { inputTokens: index * 10, outputTokens: index, totalTokens: index * 11 },
        }),
      ),
    );
    const first = await fetchProbeTokenStats(tokenQuery());
    const second = await fetchProbeTokenStats(tokenQuery({ page: 2 }));
    expect(first.total).toBe(14);
    expect(first.items).toHaveLength(10);
    expect(second.items).toHaveLength(4);
    expect(first.summary).toEqual(second.summary);
    expect(first.summary).toEqual({
      probeCount: 14,
      reportedCount: 14,
      totalReportedCount: 14,
      inputTokens: 910,
      outputTokens: 91,
      totalTokens: 1001,
      averageTokens: 71.5,
    });
    expect(first.items[0].totalTokens).toBe(143);
    expect([...first.items, ...second.items].map((item) => item.totalTokens)).toEqual(
      Array.from({ length: 14 }, (_, index) => (13 - index) * 11),
    );
  });

  it('单条删除同步改变记录和统计；尾页空了会回到有效页，健康状态及配置保持原样', async () => {
    seedProbeRecords(createProbeRecordFixtures().slice(0, 11));
    const beforeSites = JSON.stringify(sitesState);
    const before = await fetchProbeTokenStats(tokenQuery());
    const last = (await fetchProbeRecords(recordQuery({ page: 2 }))).items[0];
    expect(await deleteProbeRecord(last.id)).toBeUndefined();
    const list = await fetchProbeRecords(recordQuery({ page: 2 }));
    expect(list).toMatchObject({ page: 1, total: 10 });
    expect(list.items.some((item) => item.id === last.id)).toBe(false);
    const after = await fetchProbeTokenStats(tokenQuery());
    expect(after.summary.probeCount).toBe(before.summary.probeCount - 1);
    expect(after.summary.totalTokens).toBe(before.summary.totalTokens! - (last.totalTokens ?? 0));
    expect(JSON.stringify(sitesState)).toBe(beforeSites);
  });

  it('清理全部记录包括当前筛选之外的历史，保留健康状态、配置和筛选目录', async () => {
    const beforeSites = JSON.stringify(sitesState);
    const options = await fetchProbeRecordOptions();
    expect((await fetchProbeRecords(recordQuery({ siteId: 'site-1' }))).total).toBeLessThan(48);
    expect(await clearProbeRecords()).toBeUndefined();
    expect(await fetchProbeRecords(recordQuery())).toMatchObject({
      items: [],
      total: 0,
      retainedCount: 0,
      page: 1,
    });
    expect((await fetchProbeTokenStats(tokenQuery())).summary).toEqual({
      probeCount: 0,
      reportedCount: 0,
      totalReportedCount: 0,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      averageTokens: null,
    });
    expect(JSON.stringify(sitesState)).toBe(beforeSites);
    expect(await fetchProbeRecordOptions()).toEqual(options);
    expect(await clearProbeRecords()).toBeUndefined();
  });

  it('删除和清理失败时数据不变，读取失败拒绝 Promise 而不是伪装为空数据', async () => {
    const before = JSON.stringify(recordsState);
    window.history.replaceState({}, '', '/run/records?recordsScenario=action-error');
    await expect(deleteProbeRecord('probe-001')).rejects.toMatchObject({
      code: 'probe_records_unavailable',
      retryable: true,
    });
    await expect(clearProbeRecords()).rejects.toThrow();
    expect(JSON.stringify(recordsState)).toBe(before);
    expect((await fetchProbeRecords(recordQuery())).total).toBe(30);
    window.history.replaceState({}, '', '/run/records?recordsScenario=error');
    await expect(fetchProbeRecords(recordQuery())).rejects.toMatchObject({
      code: 'probe_records_unavailable',
    });
    await expect(fetchProbeTokenStats(tokenQuery())).rejects.toThrow();
  });

  it('一次性读取失败可以重试恢复；删除不存在的 ID 返回明确错误', async () => {
    window.history.replaceState({}, '', '/run/records?recordsScenario=retry-once');
    await expect(fetchProbeRecords(recordQuery())).rejects.toThrow();
    expect((await fetchProbeRecords(recordQuery())).total).toBe(30);
    await expect(deleteProbeRecord('not-found')).rejects.toMatchObject({
      code: 'probe_record_not_found',
      retryable: false,
    });
  });

  it.each([
    { page: 0 },
    { page: 1.5 },
    { pageSize: 1 },
    { pageSize: '10' },
    { range: ['24h'] },
    { range: 'yesterday' },
    { result: 'healthy' },
    { modelName: '' },
    { siteId: undefined },
  ])('无效查询 %j 返回校验错误，不能用旧健康状态冒充请求结果', async (invalid) => {
    await expect(
      fetchProbeRecords({ ...recordQuery(), ...invalid } as ProbeRecordsQuery),
    ).rejects.toMatchObject({
      code: 'probe_records_validation',
      category: 'validation',
      retryable: false,
    });
  });
});
