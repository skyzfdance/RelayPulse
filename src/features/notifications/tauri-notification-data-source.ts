import { invoke } from '@tauri-apps/api/core';
import type { InvokeArgs } from '@tauri-apps/api/core';
import { beginBackendOperation } from '../../app/frontend-diagnostics';
import { backendError } from '../../data/tauri-relay-pulse-data-source';
import type {
  NotificationDataSource,
  NotificationChannelSnapshot,
  NotificationSettingsSnapshot,
  NotificationSettingsUpdate,
} from './notification-types';

type InvokeNotificationCommand = <T>(command: string, args?: InvokeArgs) => Promise<T>;
type NotificationCommandArgs = Record<string, unknown>;

export class TauriNotificationDataSource implements NotificationDataSource {
  constructor(private readonly invokeCommand: InvokeNotificationCommand = invoke) {}

  getSettings(): Promise<NotificationSettingsSnapshot> {
    return this.action('get_notification_settings');
  }

  updateSettings(settings: NotificationSettingsUpdate): Promise<NotificationSettingsSnapshot> {
    return this.observedAction('update_notification_settings', { settings });
  }

  requestPermission(channelId: string): Promise<NotificationChannelSnapshot> {
    return this.observedAction('request_notification_permission', { channelId });
  }

  sendTestNotification(channelId: string): Promise<void> {
    return this.observedAction('send_test_notification', { channelId });
  }

  private async action<T>(command: string, args?: NotificationCommandArgs): Promise<T> {
    try {
      return await this.invokeCommand<T>(command, args);
    } catch (cause) {
      throw backendError(cause);
    }
  }

  private observedAction<T>(command: string, args?: NotificationCommandArgs): Promise<T> {
    return this.action(command, { ...(args ?? {}), operationId: beginBackendOperation() });
  }
}
