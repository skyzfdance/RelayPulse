import { describe, expect, it } from 'vitest';

import { createRequestParameters } from '@/utils/request-parameters';
import {
  buildIqParameters,
  iqParameterError,
  iqPresets,
  iqStartError,
} from '@/views/iq-test/iq-parameters';

const available = [{ modelName: 'model', precheck: { status: 'available' } }];
describe('IQ 独立参数与开始门槛', () => {
  it('手写嵌套参数覆盖预设并保留兄弟字段，不注入探针8-token参数', () => {
    const parameters = createRequestParameters({
      reasoning: { effort: 'low', summary: 'auto' },
      temperature: 0.2,
    });
    expect(buildIqParameters('openai-responses', 'high', parameters)).toEqual({
      reasoning: { effort: 'low', summary: 'auto' },
      temperature: 0.2,
    });
    expect(buildIqParameters('openai-compatible', 'max', [])).toEqual({ reasoning_effort: 'max' });
    expect(buildIqParameters('custom-http', 'custom', [])).toEqual({});
    expect(iqPresets('custom-http')).toHaveLength(1);
  });
  it.each(['model', 'messages', 'messages.content', 'input', 'prompt', 'system', 'stream'])(
    '系统保留路径 %s 不可覆盖',
    (field) => {
      const parameters = createRequestParameters({ [field]: 'override' });
      expect(iqParameterError(parameters)).toContain('不可覆盖');
      expect(() => buildIqParameters('openai-compatible', 'high', parameters)).toThrow();
    },
  );
  it('禁用父级阻止子字段发送，JSON/数字/危险路径错误禁止提交', () => {
    const parameters = createRequestParameters({ reasoning: { effort: 'low' } });
    parameters[0].enabled = false;
    expect(buildIqParameters('openai-responses', 'high', parameters)).toEqual({
      reasoning: { effort: 'high' },
    });
    expect(
      iqParameterError([{ key: '__proto__.polluted', value: 'x', type: 'string', enabled: true }]),
    ).toBeTruthy();
    expect(
      iqParameterError([{ key: 'temperature', value: 'NaN', type: 'number', enabled: true }]),
    ).toBeTruthy();
  });
  it.each([null, 59, 301, 120.5])('超时 %s 阻止开始', (value) => {
    expect(iqStartError(['q'], available, value, 5)).toContain('超时');
  });
  it.each([null, 4, 21, 5.5])('重试 %s 阻止开始', (value) => {
    expect(iqStartError(['q'], available, 120, value)).toContain('重试');
  });
  it('整数边界可用，缺题/参与者/未完成预检各自阻止开始', () => {
    expect(iqStartError(['q'], available, 60, 5)).toBeUndefined();
    expect(iqStartError(['q'], available, 300, 20)).toBeUndefined();
    expect(iqStartError([], available, 120, 5)).toBeTruthy();
    expect(iqStartError(['q'], [], 120, 5)).toBeTruthy();
    expect(
      iqStartError(['q'], [{ modelName: 'A', precheck: { status: 'checking' } }], 120, 5),
    ).toContain('A');
  });
});
