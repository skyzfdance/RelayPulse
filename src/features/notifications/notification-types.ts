export type NotificationPermission =
  'unknown' | 'not_requested' | 'denied' | 'granted' | 'unsupported';

export type NotificationEventTone = 'critical' | 'positive' | 'neutral';

export interface NotificationChannelSnapshot {
  id: string;
  displayName: string;
  description: string;
  permission: NotificationPermission;
  supported: boolean;
}

export interface NotificationEventCatalogItem {
  eventType: string;
  payloadVersion: number;
  category: string;
  categoryLabel?: string;
  displayName: string;
  description: string;
  defaultEnabled: boolean;
  supportedChannels: string[];
  tone?: NotificationEventTone;
}

export interface NotificationRuleSnapshot {
  eventType: string;
  enabled: boolean;
  channelIds: string[];
}

export interface NotificationSettingsSnapshot {
  revision: number;
  notificationsEnabled: boolean;
  channels: NotificationChannelSnapshot[];
  eventCatalog: NotificationEventCatalogItem[];
  rules: NotificationRuleSnapshot[];
}

export interface NotificationSettingsUpdate {
  revision: number;
  notificationsEnabled: boolean;
  rules: NotificationRuleSnapshot[];
}

export interface NotificationDataSource {
  getSettings(): Promise<NotificationSettingsSnapshot>;
  updateSettings(settings: NotificationSettingsUpdate): Promise<NotificationSettingsSnapshot>;
  sendTestNotification(channelId: string): Promise<void>;
}
