import type { ProbePolicy, RequestParameter } from '@/types/domain/sites';

import { beforeEach, describe, expect, it } from 'vitest';

import { fetchSite, savePolicy } from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { buildProbePreview, defaultProbePolicy } from '@/views/config/sites/probe-preview';

import { chatPolicy, policyWithBody } from '../../../support/parameter-fields';

beforeEach(() => {
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});

const parameter = (
  key: string,
  value: string,
  type: RequestParameter['type'] = 'string',
  enabled = true,
): RequestParameter => ({ key, value, type, enabled });
const policy = (parameters: RequestParameter[]): ProbePolicy => ({ version: 2, parameters });

describe('探针参数预览与保存契约', () => {
  it.each([
    [
      'openai-compatible',
      {
        model: '<模型名称>',
        messages: [{ role: 'user', content: '只回复 OK' }],
        stream: true,
        max_tokens: 8,
        reasoning_effort: 'low',
      },
    ],
    [
      'openai-responses',
      {
        model: '<模型名称>',
        input: '只回复 OK',
        stream: true,
        max_output_tokens: 8,
        reasoning: { effort: 'low' },
      },
    ],
    [
      'anthropic',
      {
        model: '<模型名称>',
        messages: [{ role: 'user', content: '只回复 OK' }],
        stream: true,
        max_tokens: 8,
      },
    ],
    ['custom-http', { model: '<模型名称>' }],
  ] as const)('%s 创建时载入基础参数，之后不根据协议重写', async (protocol, expected) => {
    const initial = defaultProbePolicy(protocol);
    expect(buildProbePreview('<模型名称>', protocol, initial)).toEqual(expected);
    expect(buildProbePreview('<模型名称>', 'custom-http', initial)).toEqual(expected);
    const snapshot = await fetchSite('site-1');
    expect(buildProbePreview('remote', protocol, initial).model).toBe('remote');
    expect(await fetchSite('site-1')).toEqual(snapshot);
  });

  it('允许自由配置基础字段和五种类型，模型只在绑定行启用时替换', async () => {
    const draft = policyWithBody({
      model: '',
      input: '只回复 OK',
      stream: true,
      max_tokens: 8,
      reasoning: { effort: 'low' },
      metadata: { enabled: false, options: [1, 'a'], empty: {} },
    });
    const saved = await savePolicy('site-1', null, draft, null);
    expect(buildProbePreview('remote', 'openai-compatible', saved.policy)).toEqual({
      model: 'remote',
      input: '只回复 OK',
      stream: true,
      max_tokens: 8,
      reasoning: { effort: 'low' },
      metadata: { enabled: false, options: [1, 'a'], empty: {} },
    });
    const literal = policy([parameter('model', 'specified-model')]);
    expect(buildProbePreview('remote', 'openai-responses', literal)).toEqual({
      model: 'specified-model',
    });
  });

  it('禁用父级保留子级配置并跳过类型校验；重新启用后恢复子级开关', async () => {
    const draft = policy([
      parameter('reasoning.effort', 'high'),
      parameter('reasoning.invalid', '{', 'json'),
      parameter('reasoning', '{}', 'object', false),
      parameter('model', 'fixed', 'string', false),
    ]);
    const saved = await savePolicy('site-1', null, draft, null);
    expect(saved.policy).toEqual(draft);
    expect((await fetchSite('site-1')).policy).toEqual(draft);
    expect(buildProbePreview('remote', 'openai-responses', draft)).toEqual({});
    draft.parameters[1].enabled = false;
    draft.parameters[2].enabled = true;
    expect(buildProbePreview('remote', 'openai-responses', draft)).toEqual({
      reasoning: { effort: 'high' },
    });
    draft.parameters[0].enabled = false;
    expect(buildProbePreview('remote', 'openai-responses', draft)).toEqual({ reasoning: {} });
  });

  it('空参数表可以保存，预览不重新补入默认字段', async () => {
    const saved = await savePolicy('site-1', null, policy([]), null);
    expect(saved.policy.parameters).toEqual([]);
    expect(buildProbePreview('remote', 'openai-compatible', saved.policy)).toEqual({});
  });

  it('拒绝活动参数的空字段、危险路径、重复字段、父级类型冲突和错误值', async () => {
    const before = await fetchSite('site-1');
    for (const parameters of [
      [parameter('', '')],
      [parameter('reasoning..effort', 'low')],
      [parameter('reasoning. effort', 'low')],
      [parameter('__proto__.polluted', 'yes')],
      [parameter('metadata.constructor.prototype.polluted', 'yes')],
      [parameter('n', '', 'number')],
      [parameter('stream', 'yes', 'boolean')],
      [parameter('options', '{', 'json')],
      [parameter('n', '1'), parameter('n', '2')],
      [parameter('reasoning', '{}', 'json'), parameter('reasoning.effort', 'low')],
    ]) {
      expect(() => buildProbePreview('remote', 'openai-responses', policy(parameters))).toThrow();
      await expect(savePolicy('site-1', null, policy(parameters), null)).rejects.toThrow();
    }
    expect(await fetchSite('site-1')).toEqual(before);
    expect(Object.prototype).not.toHaveProperty('polluted');
  });

  it('默认策略独立保存并深拷贝，模型独立配置保持完整', async () => {
    state.sites[0].models[0].policy = chatPolicy('model prompt', 16);
    const site = await fetchSite('site-1');
    const draft = policy([parameter('reasoning_effort', 'high')]);
    const saved = await savePolicy(site.id, null, draft, null);
    expect(saved).toEqual({ ...site, policy: draft });
    draft.parameters[0].value = 'unsaved';
    expect((await fetchSite(site.id)).policy.parameters[0].value).toBe('high');
    expect(buildProbePreview('remote', site.protocol, saved.policy)).toEqual({
      reasoning_effort: 'high',
    });
  });
});
