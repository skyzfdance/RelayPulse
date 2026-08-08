import { describe, expect, it } from 'vitest';
import type { NotificationEventCatalogItem } from './notification-types';
import {
  defaultTemplateSelection,
  effectiveTemplateSelection,
  graphemeCount,
  notificationTemplateMarkers,
  renderTemplatePreview,
  templateDescriptorFor,
  validateTemplate,
} from './notification-templates';

const failedEvent: NotificationEventCatalogItem = {
  eventType: 'probe.model_failed',
  payloadVersion: 1,
  category: 'model-health',
  displayName: '模型探测故障',
  description: '模型达到稳定故障判定时发送系统通知',
  defaultEnabled: true,
  supportedChannels: ['system'],
  tone: 'critical',
};

describe('notification templates', () => {
  it('provides stable defaults, markers and event-scoped variables', () => {
    const descriptor = templateDescriptorFor(failedEvent, 'system');

    expect(descriptor.defaultMarkerId).toBe('red_failure');
    expect(descriptor.defaultBodyTemplate).toContain('{{failure_reason}}');
    expect(descriptor.variables.map((variable) => variable.key)).toEqual([
      'occurred_at',
      'event_name',
      'site_name',
      'model_name',
      'failure_reason',
    ]);
    expect(notificationTemplateMarkers.map((marker) => marker.markerId)).toEqual(
      expect.arrayContaining([
        'none',
        'red',
        'orange',
        'yellow',
        'green',
        'blue',
        'warning',
        'failure',
        'success',
        'info',
        'reminder',
        'timer',
        'task_completed',
      ]),
    );
  });

  it('creates a generic template descriptor for a newly cataloged event', () => {
    const descriptor = templateDescriptorFor(
      {
        ...failedEvent,
        eventType: 'fixture.task_completed',
        displayName: '任务完成',
        tone: 'neutral',
      },
      'system',
    );

    expect(descriptor.defaultTitleTemplate).toBe('任务完成');
    expect(descriptor.variables.map((variable) => variable.key)).toEqual([
      'occurred_at',
      'event_name',
    ]);
  });

  it('rejects unknown and malformed placeholders without partially accepting a template', () => {
    const descriptor = templateDescriptorFor(failedEvent, 'system');
    const selection = {
      ...defaultTemplateSelection(descriptor),
      mode: 'custom' as const,
      bodyTemplate: '{{recovery_state}}',
    };

    expect(validateTemplate(descriptor, selection).body).toContain('不适用于当前事件');
    expect(validateTemplate(descriptor, { ...selection, bodyTemplate: '{{site_name}' }).body).toBe(
      '变量占位符格式不完整',
    );
    expect(
      validateTemplate(descriptor, { ...selection, bodyTemplate: '安全正文\u0000' }).body,
    ).toBe('不能包含控制字符或换行');
  });

  it('renders fixed safe examples and preserves an inactive custom draft', () => {
    const descriptor = templateDescriptorFor(failedEvent, 'system');
    const custom = {
      ...defaultTemplateSelection(descriptor),
      mode: 'custom' as const,
      markerId: 'warning',
      titleTemplate: '{{site_name}} 告警',
      bodyTemplate: '{{model_name}}：{{failure_reason}}',
    };
    const preview = renderTemplatePreview(descriptor, custom);

    expect(preview).toEqual({
      title: '⚠️ 东京主站 告警',
      body: 'GPT-5：探测请求超时',
    });
    expect(effectiveTemplateSelection(descriptor, { ...custom, mode: 'default' })).toEqual(
      defaultTemplateSelection(descriptor),
    );
  });

  it('counts user-perceived graphemes for emoji and combining characters', () => {
    expect(graphemeCount('A👨‍👩‍👧‍👦e\u0301')).toBe(3);
  });
});
