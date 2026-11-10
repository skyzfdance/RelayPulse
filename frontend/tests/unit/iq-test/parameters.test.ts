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
  it('缺题、参与者或未完成预检阻止开始，不再提供执行时限', () => {
    expect(iqStartError(['q'], available, 0)).toBeUndefined();
    expect(iqStartError([], available, 5)).toBeTruthy();
    expect(iqStartError(['q'], [], 5)).toBeTruthy();
    expect(
      iqStartError(['q'], [{ modelName: 'A', precheck: { status: 'checking' } }], 5),
    ).toContain('A');
  });
  it('重试次数为非负整数，0 表示不重试，且与等待时长无关', () => {
    for (const retries of [0, 5, 20])
      expect(iqStartError(['q'], available, retries)).toBeUndefined();
    for (const retries of [null, -1, 1.5, 21, NaN])
      expect(iqStartError(['q'], available, retries)).toContain('重试次数');
  });
});
