export type NotificationPermission =
  'unknown' | 'not_requested' | 'denied' | 'granted' | 'unsupported';

export type NotificationEventTone = 'critical' | 'positive' | 'neutral';
export type NotificationTemplateMode = 'default' | 'custom';

export interface NotificationTemplateMarker {
  markerId: string;
  symbol: string;
  accessibleName: string;
  tone?: NotificationEventTone;
  enabled: boolean;
}

export interface NotificationTemplateVariable {
  key: string;
  label: string;
  group: '通用变量' | '事件变量';
  description: string;
  example: string;
  required: boolean;
}

export interface NotificationTemplateDescriptor {
  eventType: string;
  channelId: string;
  templateSchemaVersion: number;
  defaultMarkerId: string;
  defaultTitleTemplate: string;
  defaultBodyTemplate: string;
  variables: NotificationTemplateVariable[];
}

export interface NotificationTemplateOverride {
  eventType: string;
  channelId: string;
  mode: NotificationTemplateMode;
  markerId: string;
  titleTemplate: string;
  bodyTemplate: string;
  templateSchemaVersion: number;
}

export type NotificationTemplateDraft = NotificationTemplateOverride;

export interface NotificationTemplateTestResult {
  accepted: boolean;
  renderedMarker: string;
  renderedTitle: string;
  renderedBody: string;
  operationId: string;
}

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
  templateDescriptor: NotificationTemplateDescriptor;
}

export interface NotificationRuleSnapshot {
  eventType: string;
  enabled: boolean;
  channelIds: string[];
  templateOverrides: NotificationTemplateOverride[];
}

export interface NotificationSettingsSnapshot {
  revision: number;
  notificationsEnabled: boolean;
  channels: NotificationChannelSnapshot[];
  eventCatalog: NotificationEventCatalogItem[];
  rules: NotificationRuleSnapshot[];
  markerCatalog: NotificationTemplateMarker[];
}

export interface NotificationSettingsUpdate {
  revision: number;
  notificationsEnabled: boolean;
  rules: NotificationRuleSnapshot[];
}

export interface NotificationDataSource {
  getSettings(): Promise<NotificationSettingsSnapshot>;
  updateSettings(settings: NotificationSettingsUpdate): Promise<NotificationSettingsSnapshot>;
  requestPermission(channelId: string): Promise<NotificationChannelSnapshot>;
  sendTestNotification(channelId: string): Promise<void>;
  sendTemplateTestNotification(
    draft: NotificationTemplateDraft,
  ): Promise<NotificationTemplateTestResult>;
}
