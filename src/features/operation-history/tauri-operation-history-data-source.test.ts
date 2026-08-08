import { describe, expect, it, vi } from 'vitest';
import { TauriOperationHistoryDataSource } from './tauri-operation-history-data-source';

describe('TauriOperationHistoryDataSource', () => {
  it('uses the frozen list and details command shapes', async () => {
    const page = {
      items: [
        {
          recordRef: 'record_fixture',
          occurredAt: { value: '2026-08-08T08:00:00Z', precision: 'exact' as const },
          category: 'app_lifecycle' as const,
          actionLabel: '启动 RelayPulse',
        },
      ],
      datasetVersion: 'dataset_fixture',
    };
    const details = {
      ...page.items[0],
      trigger: 'system_startup' as const,
      description: 'RelayPulse 已启动。',
      detail: { kind: 'app_lifecycle' as const, event: 'started' as const },
    };
    const invoke = vi.fn().mockResolvedValueOnce(page).mockResolvedValueOnce(details);
    const dataSource = new TauriOperationHistoryDataSource(invoke);
    const query = { limit: 10, category: 'app_lifecycle' as const };

    await expect(dataSource.listRecords(query)).resolves.toEqual(page);
    await expect(dataSource.getRecordDetails('record_fixture')).resolves.toEqual(details);

    expect(invoke).toHaveBeenNthCalledWith(1, 'list_operation_records', { query });
    expect(invoke).toHaveBeenNthCalledWith(2, 'get_operation_record_details', {
      recordRef: 'record_fixture',
    });
  });

  it('maps structured and unexpected failures to the page error contract', async () => {
    const expired = new TauriOperationHistoryDataSource(
      vi.fn().mockRejectedValue({
        code: 'cursor_expired',
        category: 'conflict',
        message: 'The operation history cursor is no longer valid.',
        retryable: true,
      }),
    );
    const unexpected = new TauriOperationHistoryDataSource(
      vi.fn().mockRejectedValue(new Error('raw failure must not be displayed')),
    );

    await expect(expired.listRecords({ limit: 10 })).rejects.toMatchObject({
      code: 'cursor_expired',
      message: '操作记录已更新，正在重新加载',
    });
    await expect(unexpected.getRecordDetails('record_fixture')).rejects.toMatchObject({
      code: 'unknown',
      message: '操作记录查询失败',
    });
  });
});
