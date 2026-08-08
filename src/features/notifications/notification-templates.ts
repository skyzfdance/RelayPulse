import type { NotificationEventCatalogItem } from './notification-types';

export type NotificationTemplateMode = 'default' | 'custom';

export interface NotificationTemplateMarker {
  markerId: string;
  symbol: string;
  accessibleName: string;
  tone?: 'critical' | 'positive' | 'neutral';
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

export interface NotificationTemplateSelection {
  eventType: string;
  channelId: string;
  mode: NotificationTemplateMode;
  markerId: string;
  titleTemplate: string;
  bodyTemplate: string;
  templateSchemaVersion: number;
}

export interface NotificationTemplateValidation {
  title?: string;
  body?: string;
}

export const notificationTemplateMarkers: NotificationTemplateMarker[] = [
  { markerId: 'none', symbol: '', accessibleName: '无标记', enabled: true },
  { markerId: 'red', symbol: '🔴', accessibleName: '红色', tone: 'critical', enabled: true },
  { markerId: 'orange', symbol: '🟠', accessibleName: '橙色', enabled: true },
  { markerId: 'yellow', symbol: '🟡', accessibleName: '黄色', enabled: true },
  { markerId: 'green', symbol: '🟢', accessibleName: '绿色', tone: 'positive', enabled: true },
  { markerId: 'blue', symbol: '🔵', accessibleName: '蓝色', tone: 'neutral', enabled: true },
  { markerId: 'warning', symbol: '⚠️', accessibleName: '警告', enabled: true },
  { markerId: 'failure', symbol: '❌', accessibleName: '失败', tone: 'critical', enabled: true },
  { markerId: 'success', symbol: '✅', accessibleName: '成功', tone: 'positive', enabled: true },
  { markerId: 'info', symbol: 'ℹ️', accessibleName: '信息', tone: 'neutral', enabled: true },
  { markerId: 'reminder', symbol: '🔔', accessibleName: '提醒', enabled: true },
  { markerId: 'timer', symbol: '⏱️', accessibleName: '计时', enabled: true },
  { markerId: 'task_completed', symbol: '☑️', accessibleName: '任务完成', enabled: true },
  {
    markerId: 'red_failure',
    symbol: '🔴',
    accessibleName: '红色故障',
    tone: 'critical',
    enabled: true,
  },
];

const commonVariables: NotificationTemplateVariable[] = [
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
    example: '模型探测故障',
    required: true,
  },
];

const eventTemplateDefinitions: Record<
  string,
  Pick<
    NotificationTemplateDescriptor,
    'defaultMarkerId' | 'defaultTitleTemplate' | 'defaultBodyTemplate'
  > & { variables: NotificationTemplateVariable[] }
> = {
  'probe.model_failed': {
    defaultMarkerId: 'red_failure',
    defaultTitleTemplate: '模型探测故障',
    defaultBodyTemplate:
      '{{site_name}} · {{model_name}}：{{failure_reason}}，发生时间：{{occurred_at}}',
    variables: [
      {
        key: 'site_name',
        label: '站点名称',
        group: '事件变量',
        description: '发生故障的站点名称',
        example: '东京主站',
        required: true,
      },
      {
        key: 'model_name',
        label: '模型名称',
        group: '事件变量',
        description: '发生故障的模型名称',
        example: 'GPT-5',
        required: true,
      },
      {
        key: 'failure_reason',
        label: '故障原因',
        group: '事件变量',
        description: '经过安全归类的故障原因',
        example: '探测请求超时',
        required: true,
      },
    ],
  },
  'probe.model_recovered': {
    defaultMarkerId: 'success',
    defaultTitleTemplate: '模型探测恢复',
    defaultBodyTemplate:
      '{{site_name}} · {{model_name}}：{{recovery_state}}，恢复时间：{{occurred_at}}',
    variables: [
      {
        key: 'site_name',
        label: '站点名称',
        group: '事件变量',
        description: '恢复可用的站点名称',
        example: '东京主站',
        required: true,
      },
      {
        key: 'model_name',
        label: '模型名称',
        group: '事件变量',
        description: '恢复可用的模型名称',
        example: 'GPT-5',
        required: true,
      },
      {
        key: 'recovery_state',
        label: '恢复状态',
        group: '事件变量',
        description: '恢复后的稳定状态',
        example: '已恢复为正常',
        required: true,
      },
    ],
  },
};

export function templateDescriptorFor(
  event: NotificationEventCatalogItem,
  channelId: string,
): NotificationTemplateDescriptor {
  const definition = eventTemplateDefinitions[event.eventType] ?? {
    defaultMarkerId:
      event.tone === 'critical' ? 'warning' : event.tone === 'positive' ? 'success' : 'info',
    defaultTitleTemplate: event.displayName,
    defaultBodyTemplate: '{{event_name}}，发生时间：{{occurred_at}}',
    variables: [],
  };
  return {
    eventType: event.eventType,
    channelId,
    templateSchemaVersion: 1,
    defaultMarkerId: definition.defaultMarkerId,
    defaultTitleTemplate: definition.defaultTitleTemplate,
    defaultBodyTemplate: definition.defaultBodyTemplate,
    variables: [
      ...commonVariables.map((variable) => ({
        ...variable,
        example: variable.key === 'event_name' ? event.displayName : variable.example,
      })),
      ...definition.variables,
    ],
  };
}

export function defaultTemplateSelection(
  descriptor: NotificationTemplateDescriptor,
): NotificationTemplateSelection {
  return {
    eventType: descriptor.eventType,
    channelId: descriptor.channelId,
    mode: 'default',
    markerId: descriptor.defaultMarkerId,
    titleTemplate: descriptor.defaultTitleTemplate,
    bodyTemplate: descriptor.defaultBodyTemplate,
    templateSchemaVersion: descriptor.templateSchemaVersion,
  };
}

export function effectiveTemplateSelection(
  descriptor: NotificationTemplateDescriptor,
  selection: NotificationTemplateSelection,
): NotificationTemplateSelection {
  return selection.mode === 'custom' ? selection : defaultTemplateSelection(descriptor);
}

export function graphemeCount(value: string) {
  const Segmenter = (
    Intl as unknown as {
      Segmenter?: new (
        locale?: string,
        options?: { granularity: 'grapheme' },
      ) => { segment(input: string): Iterable<unknown> };
    }
  ).Segmenter;
  if (!Segmenter) return Array.from(value).length;
  return Array.from(new Segmenter('zh-CN', { granularity: 'grapheme' }).segment(value)).length;
}

export function validateTemplate(
  descriptor: NotificationTemplateDescriptor,
  selection: NotificationTemplateSelection,
): NotificationTemplateValidation {
  const effective = effectiveTemplateSelection(descriptor, selection);
  const allowedVariables = new Set(descriptor.variables.map((variable) => variable.key));
  return {
    title: validateTemplateField(effective.titleTemplate, 120, false, allowedVariables),
    body: validateTemplateField(effective.bodyTemplate, 500, true, allowedVariables),
  };
}

function validateTemplateField(
  value: string,
  maximum: number,
  allowNewlines: boolean,
  allowedVariables: Set<string>,
) {
  if (!value.trim()) return '内容不能为空';
  if (graphemeCount(value) > maximum) return `不能超过 ${maximum} 个字符`;
  if (
    Array.from(value).some(
      (character) => character !== '\n' && character !== '\r' && character < ' ',
    ) ||
    (!allowNewlines && /[\r\n]/u.test(value))
  ) {
    return '不能包含控制字符或换行';
  }

  const placeholderPattern = /\{\{([a-z][a-z0-9_]*)\}\}/gu;
  for (const match of value.matchAll(placeholderPattern)) {
    if (!allowedVariables.has(match[1])) return `变量 {{${match[1]}}} 不适用于当前事件`;
  }
  if (value.replace(placeholderPattern, '').match(/[{}]/u)) return '变量占位符格式不完整';
  return undefined;
}

export function renderTemplatePreview(
  descriptor: NotificationTemplateDescriptor,
  selection: NotificationTemplateSelection,
) {
  const effective = effectiveTemplateSelection(descriptor, selection);
  const examples = new Map(
    descriptor.variables.map((variable) => [variable.key, variable.example] as const),
  );
  const render = (template: string) =>
    template.replace(/\{\{([a-z][a-z0-9_]*)\}\}/gu, (_, key: string) => examples.get(key) ?? '');
  const marker = notificationTemplateMarkers.find(
    (candidate) => candidate.markerId === effective.markerId && candidate.enabled,
  );
  const title = render(effective.titleTemplate);
  return {
    title: marker?.symbol ? `${marker.symbol} ${title}` : title,
    body: render(effective.bodyTemplate),
  };
}

export function templateSelectionsMatch(
  left: Record<string, NotificationTemplateSelection>,
  right: Record<string, NotificationTemplateSelection>,
) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function templateSelectionKey(eventType: string, channelId: string) {
  return `${eventType}:${channelId}`;
}
