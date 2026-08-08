import type {
  NotificationEventCatalogItem,
  NotificationTemplateDescriptor,
  NotificationTemplateMarker,
  NotificationTemplateMode,
  NotificationTemplateOverride,
} from './notification-types';

export type NotificationTemplateSelection = NotificationTemplateOverride;
export type {
  NotificationTemplateDescriptor,
  NotificationTemplateMarker,
  NotificationTemplateMode,
};

export interface NotificationTemplateValidation {
  title?: string;
  body?: string;
  marker?: string;
}

export function templateDescriptorFor(
  event: NotificationEventCatalogItem,
  channelId: string,
): NotificationTemplateDescriptor {
  const descriptor = event.templateDescriptor;
  if (descriptor.eventType !== event.eventType || descriptor.channelId !== channelId) {
    throw new Error('notification template catalog is inconsistent');
  }
  return descriptor;
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
  markers: NotificationTemplateMarker[],
): NotificationTemplateValidation {
  if (
    selection.eventType !== descriptor.eventType ||
    selection.channelId !== descriptor.channelId ||
    selection.templateSchemaVersion !== descriptor.templateSchemaVersion
  ) {
    return { title: '模板目录已更新，请重新加载后再编辑' };
  }
  const effective = effectiveTemplateSelection(descriptor, selection);
  const allowedVariables = new Set(descriptor.variables.map((variable) => variable.key));
  const marker = markers.find(
    (candidate) => candidate.markerId === effective.markerId && candidate.enabled,
  );
  const title = validateTemplateField(effective.titleTemplate, 120, false, allowedVariables);
  const body = validateTemplateField(effective.bodyTemplate, 500, true, allowedVariables);
  if (title || body || !marker) {
    return { title, body, marker: marker ? undefined : '标题标记不可用' };
  }
  const preview = renderTemplatePreview(descriptor, selection, markers);
  return {
    title: graphemeCount(preview.title) > 120 ? '标记和标题合计不能超过 120 个字符' : undefined,
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
  markers: NotificationTemplateMarker[],
) {
  const effective = effectiveTemplateSelection(descriptor, selection);
  const examples = new Map(
    descriptor.variables.map((variable) => [variable.key, variable.example] as const),
  );
  const render = (template: string) =>
    template.replace(/\{\{([a-z][a-z0-9_]*)\}\}/gu, (_, key: string) => examples.get(key) ?? '');
  const marker = markers.find(
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

export function isDefaultTemplateSelection(
  descriptor: NotificationTemplateDescriptor,
  selection: NotificationTemplateSelection,
) {
  return JSON.stringify(selection) === JSON.stringify(defaultTemplateSelection(descriptor));
}
