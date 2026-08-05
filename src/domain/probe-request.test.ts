import { describe, expect, it } from 'vitest';
import {
  buildCustomProbeRequest,
  buildProbeRequest,
  isParameterPathValid,
  isReservedParameter,
  isParameterValueValid,
  reservedProbeFields,
  resolveCustomProbeEndpoint,
} from './probe-request';

describe('buildProbeRequest', () => {
  it('deeply merges pass-through parameters while keeping transport fields protected', () => {
    const request = buildProbeRequest('gpt-test', 'openai-responses', {
      mode: 'minimum-reasoning',
      maxOutputTokens: 8,
      prompt: '只回复 OK',
      parameters: [
        { key: 'model', value: 'overridden', type: 'string' },
        { key: 'stream', value: 'false', type: 'boolean' },
        { key: 'reasoning.effort', value: 'max', type: 'string' },
        { key: 'max_output_tokens', value: '16', type: 'number' },
        { key: 'temperature', value: '0', type: 'number' },
        { key: 'store', value: 'false', type: 'boolean' },
      ],
    });

    expect(request).toMatchObject({
      model: 'gpt-test',
      input: '只回复 OK',
      stream: true,
      max_output_tokens: 16,
      reasoning: { effort: 'max' },
      temperature: 0,
      store: false,
    });
  });

  it('validates safe object paths and protocol-specific reserved roots', () => {
    const reserved = reservedProbeFields('openai-responses');

    expect(isParameterPathValid('reasoning.effort')).toBe(true);
    expect(isParameterPathValid('reasoning..effort')).toBe(false);
    expect(isParameterPathValid('metadata.__proto__.value')).toBe(false);
    expect(isReservedParameter('stream.enabled', reserved)).toBe(true);
    expect(isReservedParameter('reasoning.effort', reserved)).toBe(false);
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
