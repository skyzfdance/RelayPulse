import { describe, expect, it, vi } from 'vitest';
import { createNotificationFixture } from './mock-notification-data-source';
import { TauriNotificationDataSource } from './tauri-notification-data-source';

describe('TauriNotificationDataSource', () => {
  it('maps reads, settings writes and test actions to the frozen commands', async () => {
    const snapshot = createNotificationFixture();
    const invoke = vi
      .fn()
      .mockResolvedValueOnce(snapshot)
      .mockResolvedValueOnce({ ...snapshot, revision: 2 })
      .mockResolvedValueOnce(snapshot.channels[0])
      .mockResolvedValueOnce(undefined);
    const dataSource = new TauriNotificationDataSource(invoke);

    await expect(dataSource.getSettings()).resolves.toEqual(snapshot);
    await dataSource.updateSettings({
      revision: snapshot.revision,
      notificationsEnabled: false,
      rules: snapshot.rules,
    });
    await dataSource.requestPermission('system');
    await dataSource.sendTestNotification('system');

    expect(invoke).toHaveBeenNthCalledWith(1, 'get_notification_settings', undefined);
    expect(invoke).toHaveBeenNthCalledWith(2, 'update_notification_settings', {
      settings: {
        revision: 1,
        notificationsEnabled: false,
        rules: snapshot.rules,
      },
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'request_notification_permission', {
      channelId: 'system',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(4, 'send_test_notification', {
      channelId: 'system',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('preserves structured notification errors', async () => {
    const dataSource = new TauriNotificationDataSource(
      vi.fn().mockRejectedValue({
        code: 'notification_settings_conflict',
        category: 'conflict',
        message: 'Notification settings changed in another operation.',
        retryable: true,
        operationId: 'op_fixture',
      }),
    );

    await expect(
      dataSource.updateSettings({ revision: 1, notificationsEnabled: true, rules: [] }),
    ).rejects.toMatchObject({
      code: 'notification_settings_conflict',
      retryable: true,
      operationId: 'op_fixture',
    });
  });
});
