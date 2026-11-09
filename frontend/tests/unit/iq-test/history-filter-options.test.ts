import type { IqHistoryFilterOption } from '@/types/domain/iq-test';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope } from 'vue';

import { fetchIqHistoryFilterOptions } from '@/api/iq-test/history';
import { useIqHistoryFilterOptions } from '@/views/iq-test/history/use-iq-history-filter-options';

import { deferred } from '../../support/deferred';

vi.mock('@/api/iq-test/history', () => ({ fetchIqHistoryFilterOptions: vi.fn() }));

const disposals: (() => void)[] = [];
function createOptions() {
  const scope = effectScope();
  const state = scope.run(() => useIqHistoryFilterOptions())!;
  disposals.push(() => scope.stop());
  return { ...state, scope };
}
function directory(label: string): IqHistoryFilterOption[] {
  return [
    {
      value: 'retired-site',
      label,
      children: [
        { value: 'retired-site/second-model', label: '同名历史模型' },
        { value: 'retired-site/first-model', label: '同名历史模型' },
      ],
    },
  ];
}
beforeEach(() => {
  vi.mocked(fetchIqHistoryFilterOptions).mockResolvedValue([]);
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  vi.resetAllMocks();
});

describe('IQ 历史独立筛选目录', () => {
  it('直接保存接口树，保持稳定标识、名称、顺序及对象引用', async () => {
    const expected = directory('接口返回的已删除站点');
    vi.mocked(fetchIqHistoryFilterOptions).mockResolvedValueOnce(expected);
    const options = createOptions();

    expect(options.options.value).toEqual([]);
    await options.load();
    expect(fetchIqHistoryFilterOptions).toHaveBeenCalledExactlyOnceWith();
    expect(options.options.value).toBe(expected);
    expect(options.loading.value).toBe(false);
    expect(options.error.value).toBe('');
  });

  it('首次加载失败不生成替代目录，重试独立清错并显示加载状态', async () => {
    const pending = deferred<IqHistoryFilterOption[]>();
    vi.mocked(fetchIqHistoryFilterOptions)
      .mockRejectedValueOnce(new Error('筛选目录暂时不可用'))
      .mockReturnValueOnce(pending.promise);
    const options = createOptions();
    await options.load();
    expect(options.error.value).toBe('筛选目录暂时不可用');
    expect(options.options.value).toEqual([]);
    expect(options.loading.value).toBe(false);

    const retry = options.load();
    expect(options.loading.value).toBe(true);
    expect(options.error.value).toBe('');
    const expected = directory('重试后的目录');
    pending.resolve(expected);
    await retry;
    expect(options.options.value).toBe(expected);
    expect(options.loading.value).toBe(false);
    expect(fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(2);
  });

  it('刷新失败保留上次接口目录，不影响另一个页面实例的加载和错误', async () => {
    const expected = directory('已加载目录');
    const pending = deferred<IqHistoryFilterOption[]>();
    vi.mocked(fetchIqHistoryFilterOptions)
      .mockResolvedValueOnce(expected)
      .mockReturnValueOnce(pending.promise)
      .mockRejectedValueOnce(new Error('当前实例刷新失败'));
    const first = createOptions();
    const second = createOptions();
    await first.load();
    const loadingSecond = second.load();
    await first.load();

    expect(first.options.value).toBe(expected);
    expect(first.loading.value).toBe(false);
    expect(first.error.value).toBe('当前实例刷新失败');
    expect(second.options.value).toEqual([]);
    expect(second.loading.value).toBe(true);
    expect(second.error.value).toBe('');
    pending.resolve([]);
    await loadingSecond;
    expect(second.loading.value).toBe(false);
    expect(first.error.value).toBe('当前实例刷新失败');
  });

  it.each(['成功', '失败'])('迟到的旧请求%s不能覆盖较新请求的目录或错误', async (outcome) => {
    const old = deferred<IqHistoryFilterOption[]>();
    const current = deferred<IqHistoryFilterOption[]>();
    vi.mocked(fetchIqHistoryFilterOptions)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise);
    const options = createOptions();
    const first = options.load();
    const second = options.load();
    const expected = directory('最新目录');
    current.resolve(expected);
    await second;
    if (outcome === '成功') old.resolve(directory('已过期目录'));
    else old.reject(new Error('已过期错误'));
    await first;

    expect(options.options.value).toBe(expected);
    expect(options.error.value).toBe('');
    expect(options.loading.value).toBe(false);
  });

  it('旧请求先结束时不能提前结束最新请求的 loading', async () => {
    const old = deferred<IqHistoryFilterOption[]>();
    const current = deferred<IqHistoryFilterOption[]>();
    vi.mocked(fetchIqHistoryFilterOptions)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise);
    const options = createOptions();
    const first = options.load();
    const second = options.load();
    old.resolve(directory('旧目录'));
    await first;
    expect(options.loading.value).toBe(true);
    expect(options.options.value).toEqual([]);
    expect(options.error.value).toBe('');
    current.resolve(directory('新目录'));
    await second;
    expect(options.loading.value).toBe(false);
  });

  it.each(['成功', '失败'])('作用域卸载后忽略请求%s，且不能再次发起查询', async (outcome) => {
    const expected = directory('卸载前目录');
    const pending = deferred<IqHistoryFilterOption[]>();
    vi.mocked(fetchIqHistoryFilterOptions)
      .mockResolvedValueOnce(expected)
      .mockReturnValueOnce(pending.promise);
    const options = createOptions();
    await options.load();
    const loading = options.load();
    options.scope.stop();
    if (outcome === '成功') pending.resolve(directory('卸载后的目录'));
    else pending.reject(new Error('卸载后的错误'));
    await loading;
    await options.load();

    expect(options.options.value).toBe(expected);
    expect(options.error.value).toBe('');
    expect(fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(2);
  });
});
