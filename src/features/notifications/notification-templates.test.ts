import { describe, expect, it } from 'vitest';
import { createNotificationFixture } from './mock-notification-data-source';
import {
  defaultTemplateSelection,
  effectiveTemplateSelection,
  graphemeCount,
  renderTemplatePreview,
  templateDescriptorFor,
  validateTemplate,
} from './notification-templates';

const snapshot = createNotificationFixture();
const failedEvent = snapshot.eventCatalog[0];
const markers = snapshot.markerCatalog;

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
    expect(markers.map((marker) => marker.markerId)).toEqual(
      expect.arrayContaining(['none', 'red_failure', 'success', 'warning']),
    );
  });

  it('rejects an inconsistent backend event and template catalog', () => {
    expect(() =>
      templateDescriptorFor({ ...failedEvent, eventType: 'fixture.task_completed' }, 'system'),
    ).toThrow('catalog is inconsistent');
  });

  it('rejects unknown and malformed placeholders without partially accepting a template', () => {
    const descriptor = templateDescriptorFor(failedEvent, 'system');
    const selection = {
      ...defaultTemplateSelection(descriptor),
      mode: 'custom' as const,
      bodyTemplate: '{{recovery_state}}',
    };

    expect(validateTemplate(descriptor, selection, markers).body).toContain('不适用于当前事件');
    expect(
      validateTemplate(descriptor, { ...selection, bodyTemplate: '{{site_name}' }, markers).body,
    ).toBe('变量占位符格式不完整');
    expect(
      validateTemplate(descriptor, { ...selection, bodyTemplate: '安全正文\u0000' }, markers).body,
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
    const preview = renderTemplatePreview(descriptor, custom, markers);

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
