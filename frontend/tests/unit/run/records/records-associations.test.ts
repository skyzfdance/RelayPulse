import type { ProbeRecordAssociationLookup } from '@/types/domain/records';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { fetchProbeRecordsByIds } from '@/api/run/records';

const tauri = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauri.invoke,
  isTauri: () => true,
}));

beforeEach(() => {
  tauri.invoke.mockReset();
});
afterEach(() => vi.restoreAllMocks());

describe('真实探测任务关联读取', () => {
  it('超过 100 条时顺序分批完整查询，保留部分缺失，并去除重复关联', async () => {
    const ids = Array.from({ length: 205 }, (_, index) => `record-${index}`);
    let activeRequests = 0;
    let maximumActiveRequests = 0;
    tauri.invoke.mockImplementation(
      async (
        command: string,
        { recordIds }: { recordIds: string[] },
      ): Promise<ProbeRecordAssociationLookup> => {
        expect(command).toBe('get_probe_records_by_ids');
        activeRequests += 1;
        maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests);
        await Promise.resolve();
        activeRequests -= 1;
        return {
          items: recordIds
            .filter((id) => id !== 'record-103')
            .map((id) => ({ id }) as ProbeRecordAssociationLookup['items'][number]),
          missingIds: recordIds.filter((id) => id === 'record-103'),
        };
      },
    );

    const result = await fetchProbeRecordsByIds([...ids, ids[0]]);

    expect(tauri.invoke.mock.calls.map(([, args]) => args.recordIds.length)).toEqual([100, 100, 5]);
    expect(maximumActiveRequests).toBe(1);
    expect(result.items.map((item) => item.id)).toEqual(ids.filter((id) => id !== 'record-103'));
    expect(result.missingIds).toEqual(['record-103']);
  });

  it('任意一批数据库读取失败时拒绝整个查询，不把未读取记录标成缺失或返回演示数据', async () => {
    tauri.invoke
      .mockResolvedValueOnce({
        items: [],
        missingIds: Array.from({ length: 100 }, (_, index) => String(index)),
      })
      .mockRejectedValueOnce({
        code: 'probe_records_unavailable',
        category: 'storage',
        message: '无法读取探测记录',
        retryable: true,
      });

    await expect(
      fetchProbeRecordsByIds(Array.from({ length: 205 }, (_, index) => String(index))),
    ).rejects.toMatchObject({ code: 'probe_records_unavailable', category: 'storage' });
    expect(tauri.invoke).toHaveBeenCalledTimes(2);
  });

  it('空 ID 集合仍由后端严格校验', async () => {
    tauri.invoke.mockRejectedValue({
      code: 'probe_records_validation',
      category: 'validation',
      message: '关联探测记录 ID 列表无效',
      field: 'recordIds',
      retryable: false,
    });

    await expect(fetchProbeRecordsByIds([])).rejects.toMatchObject({ field: 'recordIds' });
    expect(tauri.invoke).toHaveBeenCalledWith('get_probe_records_by_ids', { recordIds: [] });
  });
});
