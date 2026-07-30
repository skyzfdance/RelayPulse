import { describe, expect, it } from 'vitest';
import {
  buildCustomProbeRequest,
  buildProbeRequest,
  isParameterValueValid,
  resolveCustomProbeEndpoint,
} from './probe-request';

describe('buildProbeRequest', () => {
  it('keeps reserved fields under RelayPulse control and converts parameter values', () => {
    const request = buildProbeRequest('gpt-test', {
      mode: 'disabled-thinking',
      maxOutputTokens: 8,
      prompt: '只回复 OK',
      parameters: [
        { key: 'model', value: 'overridden', type: 'string' },
        { key: 'stream', value: 'false', type: 'boolean' },
        { key: 'temperature', value: '0', type: 'number' },
        { key: 'store', value: 'false', type: 'boolean' },
      ],
    });

    expect(request).toMatchObject({
      model: 'gpt-test',
      stream: true,
      max_output_tokens: 8,
      temperature: 0,
      store: false,
    });
  });

  it('builds custom non-streaming requests with JSON parameters', () => {
    const request = buildCustomProbeRequest('gpt-image-1', {
      enabled: true,
      endpoint: '/v1/images/generations',
      parameters: [
        { key: 'model', value: 'overridden', type: 'string' },
        { key: 'prompt', value: '生成一个白色方块', type: 'string' },
        { key: 'n', value: '1', type: 'number' },
        { key: 'metadata', value: '{"source":"relaypulse"}', type: 'json' },
      ],
    });

    expect(request).toEqual({
      model: 'gpt-image-1',
      prompt: '生成一个白色方块',
      n: 1,
      metadata: { source: 'relaypulse' },
    });
    expect(request).not.toHaveProperty('messages');
    expect(request).not.toHaveProperty('stream');
  });

  it('validates parameter values and resolves relative custom endpoints', () => {
    expect(isParameterValueValid({ key: 'metadata', value: '{bad}', type: 'json' })).toBe(false);
    expect(isParameterValueValid({ key: 'n', value: 'not-a-number', type: 'number' })).toBe(false);
    expect(resolveCustomProbeEndpoint('https://api.openai.com/v1', '/v1/images/generations')).toBe(
      'https://api.openai.com/v1/images/generations',
    );
    expect(resolveCustomProbeEndpoint('https://api.openai.com/v1', 'responses')).toBe(
      'https://api.openai.com/v1/responses',
    );
    expect(() =>
      resolveCustomProbeEndpoint('https://api.openai.com/v1', 'https://other.example/probe'),
    ).toThrow('must use the site origin');
  });
});
