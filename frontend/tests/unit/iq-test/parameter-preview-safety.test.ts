import { describe, expect, it } from 'vitest';

import { buildIqParameters, iqParameterError } from '@/views/iq-test/iq-parameters';

const marker = 'iqPreviewPolluted';
describe('IQ 手写 JSON 参数预览', () => {
  it.each(['__proto__', 'constructor', 'prototype'])(
    '在合并前拒绝嵌套危险键 %s，不污染对象原型',
    (key) => {
      const parameters = [
        {
          key: 'metadata',
          value: `{ "${key}": { "${marker}": "unsafe" } }`,
          type: 'json' as const,
          enabled: true,
        },
      ];
      try {
        expect(iqParameterError(parameters)).toBeTruthy();
        expect(() => buildIqParameters('openai-compatible', 'high', parameters)).toThrow();
        expect(Object.prototype).not.toHaveProperty(marker);
      } finally {
        Reflect.deleteProperty(Object.prototype, marker);
      }
    },
  );
});
