import type { OperationLogQuery } from '@/types/domain/operation-logs';

import { afterEach, describe, expect, it, vi } from 'vitest';
import { effectScope } from 'vue';

import * as operationApi from '@/api/run/operation-logs';
import { fetchOperationLogDetail, fetchOperationLogs } from '@/api/run/operation-logs';
import * as recordApi from '@/api/run/records';
import {
  parseOperationLogQuery,
  serializeOperationLogQuery,
} from '@/views/run/operation-history/operation-log-query';
import { useOperationLogs } from '@/views/run/operation-history/use-operation-logs';

import { deferred } from '../../../support/deferred';

const baseQuery: OperationLogQuery = {
  startDate: null,
  endDate: null,
  source: [],
  result: [],
  actions: [],
  page: 1,
  pageSize: 10,
};

afterEach(() => {
  vi.restoreAllMocks();
  window.history.replaceState({}, '', '/');
});

describe('operation history query and mock contract', () => {
  it('preserves filters, page, and page size through the route query', () => {
    const query = parseOperationLogQuery({
      startDate: '2026-11-03',
      endDate: '2026-11-04',
      source: 'system,user',
      result: ['cancelled', 'failure'],
      actions: ['notification', 'probe'],
      page: '3',
      pageSize: '50',
    });
    expect(parseOperationLogQuery(serializeOperationLogQuery(query))).toEqual(query);
    expect(parseOperationLogQuery({ source: 'unknown', page: '0', pageSize: '17' })).toMatchObject({
      source: [],
      page: 1,
      pageSize: 20,
    });
  });

  it('filters before paging and clamps a page beyond the filtered total', async () => {
    const firstPage = await fetchOperationLogs({ ...baseQuery, source: ['user'], page: 1 });
    const secondPage = await fetchOperationLogs({ ...baseQuery, source: ['user'], page: 2 });
    expect(firstPage.total).toBeGreaterThan(10);
    expect(firstPage.items).toHaveLength(10);
    expect(secondPage.items[0].id).not.toBe(firstPage.items[0].id);
    expect(secondPage.items.every((item) => item.source === 'user')).toBe(true);

    const filtered = await fetchOperationLogs({
      ...baseQuery,
      source: ['system', 'user'],
      result: ['cancelled', 'failure'],
      actions: ['notification', 'probe'],
      page: 99,
    });
    expect(filtered.page).toBe(1);
    expect(filtered.items.every((item) => ['cancelled', 'failure'].includes(item.result))).toBe(
      true,
    );
    expect(
      filtered.items.every((item) => ['notification', 'probe'].includes(item.actionGroup)),
    ).toBe(true);
    expect(filtered.items.some((item) => item.actionGroup === 'notification')).toBe(true);
    expect(filtered.items.some((item) => item.actionGroup === 'probe')).toBe(true);
  });

  it('groups changes separately from observed model state and preserves actions on failure', async () => {
    const all = await fetchOperationLogs({ ...baseQuery, pageSize: 100 });
    const item = (id: string) => all.items.find((log) => log.id === id)!;
    expect(item('op-015').actionGroup).toBe('update');
    expect(item('op-023').actionGroup).toBe('update');
    expect(item('op-031').actionGroup).toBe('update');
    expect(item('op-003').actionGroup).toBe('status_change');
    expect(item('op-014').actionGroup).toBe('status_change');
    expect(item('op-027')).toMatchObject({ actionGroup: 'cleanup', result: 'failure' });
    expect(item('op-034')).toMatchObject({ actionGroup: 'probe', result: 'failure' });
    expect(item('op-025')).toMatchObject({ actionGroup: 'probe', result: 'blocked' });
    expect(item('op-002').summary).toContain('ai.input.im');
    expect(item('op-009').summary).toContain('doudui.cc');
    expect(item('op-009').summary).toContain('GPT-6 Astra');
    expect(item('op-022').summary).toContain('old-api.example');
    expect(item('op-034').summary).toContain('凭据缺失');

    const changes = await fetchOperationLogs({ ...baseQuery, actions: ['update'], pageSize: 100 });
    expect(changes.items.map((log) => log.id)).toEqual(
      expect.arrayContaining(['op-015', 'op-023', 'op-031']),
    );
    expect(changes.items.every((log) => log.actionGroup === 'update')).toBe(true);
  });

  it('normalizes former event filters to the grouped action without duplicates', () => {
    expect(
      parseOperationLogQuery({ actions: ['automatic_probe,manual_probe', 'site_reordered'] })
        .actions,
    ).toEqual(['probe', 'update']);
  });

  it('returns explicit detail behavior and reads larger content on demand', async () => {
    const page = await fetchOperationLogs(baseQuery);
    expect(page.items[0].detailBehavior).toEqual({
      kind: 'probe_records',
      recordIds: ['probe-002', 'probe-003', 'probe-004'],
      availability: 'available',
    });
    const detail = await fetchOperationLogDetail('op-002');
    expect(detail.content.kind).toBe('changes');
    expect(detail.item.objectSnapshot).toBe('ai.input.im');
  });

  it('supports empty results, a retry after query failure, and unavailable details', async () => {
    window.history.replaceState({}, '', '/?operationScenario=empty');
    expect(await fetchOperationLogs(baseQuery)).toMatchObject({ total: 0, items: [], page: 1 });
    window.history.replaceState({}, '', '/?operationScenario=retry-once');
    await expect(fetchOperationLogs(baseQuery)).rejects.toMatchObject({ category: 'unavailable' });
    expect((await fetchOperationLogs(baseQuery)).total).toBeGreaterThan(20);
    window.history.replaceState({}, '', '/?operationScenario=detail-missing');
    await expect(fetchOperationLogDetail('op-002')).rejects.toMatchObject({
      category: 'not_found',
    });
  });

  it('rejects invalid calendar dates and applies a local full-day range', async () => {
    await expect(
      fetchOperationLogs({ ...baseQuery, startDate: '2026-02-31' }),
    ).rejects.toMatchObject({ category: 'validation' });
    const page = await fetchOperationLogs(baseQuery);
    const first = new Date(page.items[0].occurredAt);
    const date = `${first.getFullYear()}-${String(first.getMonth() + 1).padStart(2, '0')}-${String(first.getDate()).padStart(2, '0')}`;
    const filtered = await fetchOperationLogs({ ...baseQuery, startDate: date, endDate: date });
    expect(filtered.items.map((item) => item.id)).toContain(page.items[0].id);
    expect(parseOperationLogQuery({ startDate: 'bad', endDate: '2026-02-31' })).toMatchObject({
      startDate: null,
      endDate: null,
    });
  });

  it('ignores a late detail response after the user switches to another log', async () => {
    const first = deferred<Awaited<ReturnType<typeof operationApi.fetchOperationLogDetail>>>();
    const second = deferred<Awaited<ReturnType<typeof operationApi.fetchOperationLogDetail>>>();
    const read = vi.spyOn(operationApi, 'fetchOperationLogDetail');
    read.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const scope = effectScope();
    const state = scope.run(() => useOperationLogs())!;

    const oldRequest = state.loadDetail('op-old');
    const newRequest = state.loadDetail('op-new');
    second.resolve({
      item: { id: 'op-new' } as never,
      explanation: 'new',
      content: { kind: 'snapshot', fields: [] },
    });
    await newRequest;
    first.resolve({
      item: { id: 'op-old' } as never,
      explanation: 'old',
      content: { kind: 'snapshot', fields: [] },
    });
    await oldRequest;
    expect(state.detail.value?.item.id).toBe('op-new');
    scope.stop();
  });

  it('does not publish stale list responses or late failures after a filter change', async () => {
    const first = deferred<Awaited<ReturnType<typeof operationApi.fetchOperationLogs>>>();
    const second = deferred<Awaited<ReturnType<typeof operationApi.fetchOperationLogs>>>();
    vi.spyOn(operationApi, 'fetchOperationLogs')
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const scope = effectScope();
    const state = scope.run(() => useOperationLogs())!;
    const oldRequest = state.load({ ...baseQuery, page: 2 });
    const newRequest = state.load({ ...baseQuery, source: ['user'] });
    second.resolve({ items: [], total: 0, page: 1, pageSize: 10 });
    await newRequest;
    first.reject(new Error('old failure'));
    expect(await oldRequest).toBeUndefined();
    expect(state.data.value?.page).toBe(1);
    expect(state.error.value).toBe('');
    scope.stop();
  });
});

describe('probe record association lookup', () => {
  it('resolves only the exact requested record IDs and reports missing records', async () => {
    const result = await recordApi.fetchProbeRecordsByIds([
      'probe-003',
      'deleted-record',
      'probe-001',
    ]);
    expect(result.items.map((item) => item.id)).toEqual(['probe-003', 'probe-001']);
    expect(result.missingIds).toEqual(['deleted-record']);
  });
});
