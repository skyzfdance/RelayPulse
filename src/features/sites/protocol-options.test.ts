import { describe, expect, it } from 'vitest';
import { siteProtocolOptions } from './protocol-options';

describe('siteProtocolOptions', () => {
  it('exposes the three fixed protocols followed by custom HTTP', () => {
    expect(siteProtocolOptions).toEqual([
      { value: 'openai-compatible', label: 'OpenAI 兼容协议' },
      { value: 'anthropic', label: 'Anthropic 协议' },
      { value: 'openai-responses', label: 'OpenAI Responses 协议' },
      { value: 'custom-http', label: '自定义 HTTP' },
    ]);
  });
});
