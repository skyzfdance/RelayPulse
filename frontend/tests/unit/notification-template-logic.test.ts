import type { NotificationMarker, NotificationVariable } from '@/types/domain/notifications';

import { describe, expect, it } from 'vitest';

import {
  graphemeCount,
  renderTemplatePreview,
  validateNotificationPreview,
  validateTemplate,
} from '@/views/config/notifications/template-logic';

const none: NotificationMarker = { markerId: 'none', symbol: '', name: '无标记' };
const warning: NotificationMarker = { markerId: 'warning', symbol: '⚠️', name: '警告' };
const variables: NotificationVariable[] = [
  { key: 'app', label: '应用名', description: '', example: 'RelayPulse', group: '通用变量' },
  { key: 'model', label: '模型', description: '', example: 'gpt-4o', group: '事件变量' },
];

function validate(title: string, body = '正文', marker = none) {
  return validateTemplate(title, body, variables, marker);
}

function valid(result: ReturnType<typeof validate>) {
  expect(Object.values(result).filter(Boolean)).toEqual([]);
}

describe('通知文本与后端校验规则一致', () => {
  it('拒绝 C0、DEL、C1 控制字符，正文仅豁免 CR/LF', () => {
    for (const code of [
      ...Array.from({ length: 32 }, (_, index) => index),
      ...Array.from({ length: 33 }, (_, index) => 0x7f + index),
    ]) {
      const text = `文字${String.fromCodePoint(code)}内容`;
      expect(validate(text).title, `标题 U+${code.toString(16)}`).toBeDefined();
      const body = validate('标题', text).body;
      if (code === 10 || code === 13) expect(body).toBeUndefined();
      else expect(body, `正文 U+${code.toString(16)}`).toBeDefined();
      const preview = validateNotificationPreview({ title: text, body: text });
      expect(preview.title).toBeDefined();
      expect(preview.body).toBe(body);
    }
  });

  it.each(['\n', '\r', '\r\n', '\u2028', '\u2029'])('标题拒绝换行 %j，正文允许', (newline) => {
    const text = `第一行${newline}第二行`;
    expect(validate(text).title).toBe('不能包含控制字符或换行');
    valid(validate('标题', text));
    expect(validateNotificationPreview({ title: text, body: text })).toEqual({
      title: '不能包含控制字符或换行',
      body: undefined,
    });
  });

  it.each(['', '   ', '\u00a0', '\u3000', '\r\n'])('拒绝空白内容 %j', (text) => {
    expect(validate(text, text)).toMatchObject({ title: '内容不能为空', body: '内容不能为空' });
    expect(validateNotificationPreview({ title: text, body: text })).toEqual({
      title: '内容不能为空',
      body: '内容不能为空',
    });
  });

  it('空白分类遵循 Rust 的 Unicode White_Space，不额外删除 BOM 或 emoji 连接符', () => {
    valid(validate('\ufeff', '\ufeff'));
    valid(validate('👩🏽‍💻', '👨‍👩‍👧‍👦'));
  });

  it.each(['👩🏽‍💻', '🇨🇳', 'e\u0301'])('按 grapheme 校验 %s 的 120/500 边界', (symbol) => {
    expect(graphemeCount(symbol)).toBe(1);
    valid(validate(symbol.repeat(120), symbol.repeat(500)));
    expect(validate(symbol.repeat(121)).title).toBeDefined();
    expect(validate('标题', symbol.repeat(501)).body).toBeDefined();
    valid(validateNotificationPreview({ title: symbol.repeat(120), body: symbol.repeat(500) }));
    expect(
      validateNotificationPreview({ title: symbol.repeat(121), body: symbol.repeat(501) }),
    ).toEqual({
      title: '不能超过 120 个字符',
      body: '不能超过 500 个字符',
    });
  });

  it('标题上限包含标记和空格，并检查变量展开后的长度', () => {
    valid(validate('👩🏽‍💻'.repeat(118), '正文', warning));
    expect(validate('👩🏽‍💻'.repeat(119), '正文', warning).title).toBe(
      '标记和标题合计不能超过 120 个字符',
    );
    valid(validate('{{app}}'.repeat(12)));
    expect(validate('{{app}}'.repeat(12) + 'x').title).toBeDefined();
  });

  it('保留变量白名单、完整占位符和标记检查', () => {
    valid(validate('{{model}}', '{{app}}'));
    for (const text of [
      '{{secret}}',
      '{{Model}}',
      '{{ app }}',
      '{{}}',
      '{{app}',
      '{app}',
      '正文}',
    ]) {
      expect(validate(text).title).toBeDefined();
      expect(validate('标题', text).body).toBeDefined();
    }
    expect(validateTemplate('标题', '正文', variables, undefined).marker).toBe('标题标记不可用');
  });

  it('允许保存源正文合法的模板，只在测试发送时拦截展开超长的正文', () => {
    const body = '{{app}}'.repeat(50);
    valid(validate('标题', body));
    valid(validateNotificationPreview(renderTemplatePreview('标题', body, variables, none)));
    valid(validate('标题', body + '!'));
    expect(
      validateNotificationPreview(renderTemplatePreview('标题', body + '!', variables, none)).body,
    ).toBe('不能超过 500 个字符');
  });

  it('成品文字保留花括号和标记，不重新解释为模板', () => {
    const preview = { title: '⚠️ {{literal}}', body: '正文 {literal}\r\n第二行' };
    valid(validateNotificationPreview(preview));
    expect(preview).toEqual({ title: '⚠️ {{literal}}', body: '正文 {literal}\r\n第二行' });
  });
});
