import type { ProbeRecordListItem, RecordsPage, TokenStatsPage } from '@/types/domain/records';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, ref } from 'vue';

import * as api from '@/api/run/records';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { useProbeRecords } from '@/views/run/records/use-probe-records';
import { useTokenStats } from '@/views/run/records/use-token-stats';

import { deferred } from '../../../support/deferred';

const disposals: (() => void)[] = [];
function createLists() {
  const scope = effectScope();
  const filters = { range: ref<'24h' | '7d'>('24h'), siteId: ref('') };
  const lists = scope.run(() => ({
    records: useProbeRecords(filters),
    tokens: useTokenStats(filters),
  }))!;
  disposals.push(() => scope.stop());
  return { ...lists, filters, scope };
}
function recordPage(name: string): RecordsPage<ProbeRecordListItem> {
  return {
    items: [
      {
        id: 'probe-001',
        siteName: name,
        modelName: 'GPT-6 Astra',
        probedAt: new Date().toISOString(),
        result: 'success',
        resultMessage: '请求成功',
        firstTokenMs: 100,
        durationMs: 200,
        totalTokens: 12345,
      },
    ],
    total: 1,
    retainedCount: 1,
    page: 1,
  };
}
function tokenPage(name: string): TokenStatsPage {
  const counts = {
    probeCount: 7,
    reportedCount: 2,
    totalReportedCount: 1,
    inputTokens: null,
    outputTokens: 0,
    totalTokens: 12345,
    averageTokens: 12345,
  };
  return {
    items: [{ siteId: 'site-1', siteName: name, ...counts }],
    summary: counts,
    total: 1,
    retainedCount: 7,
    page: 1,
  };
}
beforeEach(() => {
  window.history.replaceState({}, '', '/run/records');
  seedProbeRecords();
  resetFeedbackForTests();
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  resetFeedbackForTests();
  window.history.replaceState({}, '', '/');
});

describe('两个 Tab 的独立页面 composable', () => {
  it('各 Tab 只发送实际筛选和分页参数', async () => {
    const recordRead = vi.spyOn(api, 'fetchProbeRecords');
    const tokenRead = vi.spyOn(api, 'fetchProbeTokenStats');
    const { records, tokens, filters } = createLists();
    filters.siteId.value = 'site-1';
    records.modelName.value = 'GPT-6 Astra';
    records.result.value = 'slow';
    await Promise.all([records.load(), tokens.load()]);
    expect(recordRead).toHaveBeenCalledExactlyOnceWith({
      range: '24h',
      siteId: 'site-1',
      modelName: 'GPT-6 Astra',
      result: 'slow',
      page: 1,
      pageSize: 10,
    });
    expect(tokenRead).toHaveBeenCalledExactlyOnceWith({
      range: '24h',
      siteId: 'site-1',
      page: 1,
      pageSize: 10,
    });
  });

  it.each(['成功', '失败'])(
    '记录的较早请求%s不能覆盖新筛选，也不能提前结束 loading',
    async (outcome) => {
      const old = deferred<RecordsPage<ProbeRecordListItem>>();
      const current = deferred<RecordsPage<ProbeRecordListItem>>();
      vi.spyOn(api, 'fetchProbeRecords')
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const { records } = createLists();
      const first = records.load();
      records.modelName.value = 'GPT-6 Astra';
      const second = records.load();
      if (outcome === '成功') old.resolve(recordPage('旧请求'));
      else old.reject(new Error('旧请求错误'));
      await first;
      expect(records.loading.value).toBe(true);
      expect(records.data.value).toBeUndefined();
      expect(records.error.value).toBe('');
      current.resolve(recordPage('新请求'));
      await second;
      expect(records.data.value?.items[0].siteName).toBe('新请求');
      expect(records.loading.value).toBe(false);
    },
  );

  it.each(['成功', '失败'])(
    '统计的较早请求%s不能覆盖新筛选，统计 DTO 保持后端返回值',
    async (outcome) => {
      const old = deferred<TokenStatsPage>();
      const current = deferred<TokenStatsPage>();
      vi.spyOn(api, 'fetchProbeTokenStats')
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const { tokens, filters } = createLists();
      const first = tokens.load();
      filters.siteId.value = 'site-1';
      const second = tokens.load();
      if (outcome === '成功') old.resolve(tokenPage('旧统计'));
      else old.reject(new Error('旧统计错误'));
      await first;
      expect(tokens.loading.value).toBe(true);
      expect(tokens.data.value).toBeUndefined();
      expect(tokens.error.value).toBe('');
      const next = tokenPage('新统计');
      current.resolve(next);
      await second;
      expect(tokens.data.value).toBe(next);
      expect(tokens.loading.value).toBe(false);
    },
  );

  it('各 Tab 和不同页面实例的查询、页码、加载与错误互不覆盖', async () => {
    const first = createLists();
    const second = createLists();
    await Promise.all([first.records.load(), first.tokens.load()]);
    const previousTokens = first.tokens.data.value;
    vi.spyOn(api, 'fetchProbeRecords').mockRejectedValueOnce(new Error('仅记录读取失败'));
    await first.records.load();
    expect(first.records.error.value).toBe('仅记录读取失败');
    expect(first.tokens.error.value).toBe('');
    expect(first.tokens.data.value).toBe(previousTokens);
    expect(second.records.data.value).toBeUndefined();
    expect(second.tokens.data.value).toBeUndefined();
    first.records.paging.page = 3;
    expect(first.tokens.paging.page).toBe(1);
    expect(second.records.paging.page).toBe(1);
    expect(second.records.loading.value).toBe(false);
    expect(second.records.error.value).toBe('');
  });

  it('删除使旧记录响应失效；统计自行失效后也不能写回删除前的用量', async () => {
    const { records, tokens } = createLists();
    await Promise.all([records.load(), tokens.load()]);
    const previousTokens = tokens.data.value!;
    const oldRecords = deferred<RecordsPage<ProbeRecordListItem>>();
    const oldTokens = deferred<TokenStatsPage>();
    vi.spyOn(api, 'fetchProbeRecords').mockReturnValueOnce(oldRecords.promise);
    vi.spyOn(api, 'fetchProbeTokenStats').mockReturnValueOnce(oldTokens.promise);
    const loadingRecords = records.load();
    const loadingTokens = tokens.load();
    expect(await records.removeRecord('probe-001')).toBe(true);
    expect(tokens.data.value).toBe(previousTokens);
    tokens.invalidate(true);
    oldRecords.resolve(recordPage('删除前的旧数据'));
    oldTokens.resolve(previousTokens);
    await Promise.all([loadingRecords, loadingTokens]);
    expect(records.data.value).toBeUndefined();
    expect(tokens.data.value).toBeUndefined();
  });

  it('重复删除或同时清理只发起一次写请求，失败保留各自的数据', async () => {
    const { records, tokens } = createLists();
    await Promise.all([records.load(), tokens.load()]);
    const previousRecords = records.data.value;
    const previousTokens = tokens.data.value;
    const pending = deferred<void>();
    const remove = vi.spyOn(api, 'deleteProbeRecord').mockReturnValueOnce(pending.promise);
    const clear = vi.spyOn(api, 'clearProbeRecords');
    const deleting = records.removeRecord('probe-001');
    expect(records.mutation.value).toBe('delete');
    expect(await records.removeRecord('probe-001')).toBe(false);
    expect(await records.clearRecords()).toBe(false);
    expect(remove).toHaveBeenCalledOnce();
    expect(clear).not.toHaveBeenCalled();
    pending.reject(new Error('写入失败'));
    expect(await deleting).toBe(false);
    expect(records.data.value).toBe(previousRecords);
    expect(tokens.data.value).toBe(previousTokens);
    expect(records.busy.value).toBe(false);
  });

  it('离开页面后两个 Tab 的旧请求都失效，新实例读取失败后可以重试', async () => {
    const first = createLists();
    const pendingRecords = deferred<RecordsPage<ProbeRecordListItem>>();
    const pendingTokens = deferred<TokenStatsPage>();
    vi.spyOn(api, 'fetchProbeRecords')
      .mockReturnValueOnce(pendingRecords.promise)
      .mockRejectedValueOnce(new Error('读取失败'));
    vi.spyOn(api, 'fetchProbeTokenStats').mockReturnValueOnce(pendingTokens.promise);
    const loading = Promise.all([first.records.load(), first.tokens.load()]);
    first.scope.stop();
    const second = createLists();
    pendingRecords.resolve(recordPage('已离开的页面'));
    pendingTokens.resolve(tokenPage('已离开的统计'));
    await loading;
    expect(first.records.data.value).toBeUndefined();
    expect(first.tokens.data.value).toBeUndefined();
    expect(second.records.data.value).toBeUndefined();
    await second.records.load();
    expect(second.records.error.value).toBe('读取失败');
    expect(second.records.loading.value).toBe(false);
    await second.records.load();
    expect(second.records.error.value).toBe('');
    expect(second.records.data.value?.items).toHaveLength(10);
  });
});
