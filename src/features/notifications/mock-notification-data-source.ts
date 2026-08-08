import type {
  NotificationDataSource,
  NotificationPermission,
  NotificationSettingsSnapshot,
  NotificationSettingsUpdate,
  NotificationTemplateDescriptor,
  NotificationTemplateDraft,
} from './notification-types';

export type NotificationMockScenario =
  | 'default'
  | 'loading'
  | 'load-error'
  | 'empty'
  | 'save-error'
  | 'test-error'
  | 'permission-unknown'
  | 'permission-not-requested'
  | 'permission-denied'
  | 'permission-unsupported'
  | 'long-text';

const scenarios: NotificationMockScenario[] = [
  'default',
  'loading',
  'load-error',
  'empty',
  'save-error',
  'test-error',
  'permission-unknown',
  'permission-not-requested',
  'permission-denied',
  'permission-unsupported',
  'long-text',
];

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));
}

function cloneSnapshot(snapshot: NotificationSettingsSnapshot): NotificationSettingsSnapshot {
  return {
    ...snapshot,
    channels: snapshot.channels.map((channel) => ({ ...channel })),
    eventCatalog: snapshot.eventCatalog.map((event) => ({
      ...event,
      supportedChannels: [...event.supportedChannels],
      templateDescriptor: {
        ...event.templateDescriptor,
        variables: event.templateDescriptor.variables.map((variable) => ({ ...variable })),
      },
    })),
    rules: snapshot.rules.map((rule) => ({
      ...rule,
      channelIds: [...rule.channelIds],
      templateOverrides: rule.templateOverrides.map((template) => ({ ...template })),
    })),
    markerCatalog: snapshot.markerCatalog.map((marker) => ({ ...marker })),
  };
}

function permissionForScenario(scenario: NotificationMockScenario): NotificationPermission {
  if (scenario === 'permission-unknown') return 'unknown';
  if (scenario === 'permission-not-requested') return 'not_requested';
  if (scenario === 'permission-denied') return 'denied';
  if (scenario === 'permission-unsupported') return 'unsupported';
  return 'granted';
}

export function createNotificationFixture(
  scenario: NotificationMockScenario = 'default',
): NotificationSettingsSnapshot {
  const permission = permissionForScenario(scenario);
  const eventCatalog = [
    {
      eventType: 'probe.model_failed',
      payloadVersion: 1,
      category: 'model-availability',
      categoryLabel: '模型可用性',
      displayName: '模型探测故障',
      description: '稳定状态由非红色转为红色',
      defaultEnabled: true,
      supportedChannels: ['system'],
      tone: 'critical' as const,
      templateDescriptor: templateDescriptor('probe.model_failed'),
    },
    {
      eventType: 'probe.model_recovered',
      payloadVersion: 1,
      category: 'model-availability',
      categoryLabel: '模型可用性',
      displayName: '模型探测恢复',
      description: '稳定状态由红色恢复为正常或响应偏慢',
      defaultEnabled: true,
      supportedChannels: ['system'],
      tone: 'positive' as const,
      templateDescriptor: templateDescriptor('probe.model_recovered'),
    },
  ];

  if (scenario === 'long-text') {
    eventCatalog[0] = {
      ...eventCatalog[0],
      displayName: '跨区域模型探测连续失败并达到稳定故障判定阈值的通知事件',
      description:
        '当一个名称较长的站点中，模型稳定状态由非红色转为红色时发送；说明文字应自然换行且不能挤压渠道与事件规则开关。',
    };
  }

  return {
    revision: 1,
    notificationsEnabled: true,
    channels: [
      {
        id: 'system',
        displayName: '系统通知',
        description: '通过当前操作系统显示 RelayPulse 通知',
        permission,
        supported: permission !== 'unsupported',
      },
    ],
    eventCatalog: scenario === 'empty' ? [] : eventCatalog,
    rules: eventCatalog.map((event) => ({
      eventType: event.eventType,
      enabled: event.defaultEnabled,
      channelIds: [...event.supportedChannels],
      templateOverrides: [],
    })),
    markerCatalog: [
      { markerId: 'none', symbol: '', accessibleName: '无标记', enabled: true },
      {
        markerId: 'red_failure',
        symbol: '🔴',
        accessibleName: '红色故障',
        tone: 'critical',
        enabled: true,
      },
      {
        markerId: 'success',
        symbol: '✅',
        accessibleName: '成功',
        tone: 'positive',
        enabled: true,
      },
      { markerId: 'warning', symbol: '⚠️', accessibleName: '警告', enabled: true },
    ],
  };
}

function templateDescriptor(eventType: string): NotificationTemplateDescriptor {
  const failed = eventType === 'probe.model_failed';
  return {
    eventType,
    channelId: 'system',
    templateSchemaVersion: 1,
    defaultMarkerId: failed ? 'red_failure' : 'success',
    defaultTitleTemplate: failed ? '模型探测故障' : '模型探测恢复',
    defaultBodyTemplate: failed
      ? '{{site_name}} · {{model_name}}：{{failure_reason}}，发生时间：{{occurred_at}}'
      : '{{site_name}} · {{model_name}}：{{recovery_state}}，恢复时间：{{occurred_at}}',
    variables: [
      {
        key: 'occurred_at',
        label: '发生时间',
        group: '通用变量',
        description: '事件发生的本地时间',
        example: '2026-08-08 14:32:18',
        required: true,
      },
      {
        key: 'event_name',
        label: '事件名称',
        group: '通用变量',
        description: '当前通知事件的显示名称',
        example: failed ? '模型探测故障' : '模型探测恢复',
        required: true,
      },
      {
        key: 'site_name',
        label: '站点名称',
        group: '事件变量',
        description: '站点名称',
        example: '东京主站',
        required: true,
      },
      {
        key: 'model_name',
        label: '模型名称',
        group: '事件变量',
        description: '模型名称',
        example: 'GPT-5',
        required: true,
      },
      {
        key: failed ? 'failure_reason' : 'recovery_state',
        label: failed ? '故障原因' : '恢复状态',
        group: '事件变量',
        description: failed ? '经过安全归类的故障原因' : '恢复后的稳定状态',
        example: failed ? '探测请求超时' : '已恢复为正常',
        required: true,
      },
    ],
  };
}

export function notificationScenarioFromSearch(search: string): NotificationMockScenario {
  const requested = new URLSearchParams(search).get('notificationScenario');
  return scenarios.includes(requested as NotificationMockScenario)
    ? (requested as NotificationMockScenario)
    : 'default';
}

export class MockNotificationDataSource implements NotificationDataSource {
  private snapshot: NotificationSettingsSnapshot;

  constructor(private readonly scenario: NotificationMockScenario = 'default') {
    this.snapshot = createNotificationFixture(scenario);
  }

  async getSettings() {
    if (this.scenario === 'loading') return new Promise<NotificationSettingsSnapshot>(() => {});
    await delay(120);
    if (this.scenario === 'load-error') throw new Error('通知设置暂时无法读取');
    return cloneSnapshot(this.snapshot);
  }

  async updateSettings(settings: NotificationSettingsUpdate) {
    await delay(360);
    if (this.scenario === 'save-error') throw new Error('通知设置保存失败');

    const nextRevision = this.snapshot.revision + 1;
    this.snapshot = {
      ...this.snapshot,
      revision: nextRevision,
      notificationsEnabled: settings.notificationsEnabled,
      rules: settings.rules.map((rule) => ({ ...rule, channelIds: [...rule.channelIds] })),
    };
    return cloneSnapshot(this.snapshot);
  }

  async requestPermission(channelId: string) {
    await delay(320);
    const channel = this.snapshot.channels.find((candidate) => candidate.id === channelId);
    if (!channel) throw new Error('通知渠道不存在');
    if (!channel.supported || channel.permission === 'unknown') {
      throw new Error('系统通知当前不可用');
    }

    const permission = channel.permission === 'not_requested' ? 'granted' : channel.permission;
    this.snapshot = {
      ...this.snapshot,
      channels: this.snapshot.channels.map((candidate) =>
        candidate.id === channelId ? { ...candidate, permission } : candidate,
      ),
    };
    return { ...channel, permission };
  }

  async sendTestNotification(channelId: string) {
    await delay(420);
    if (this.scenario === 'test-error') throw new Error('测试通知发送失败');

    const channel = this.snapshot.channels.find((candidate) => candidate.id === channelId);
    if (!this.snapshot.notificationsEnabled) {
      throw new Error('通知服务未启用');
    }
    if (!channel) throw new Error('通知渠道不存在');
    if (!channel.supported || channel.permission !== 'granted') {
      throw new Error('系统通知当前不可用');
    }
  }

  async sendTemplateTestNotification(draft: NotificationTemplateDraft) {
    await this.sendTestNotification(draft.channelId);
    return {
      accepted: true,
      renderedMarker: '',
      renderedTitle: draft.titleTemplate,
      renderedBody: draft.bodyTemplate,
      operationId: 'op_mock_template_test',
    };
  }
}
