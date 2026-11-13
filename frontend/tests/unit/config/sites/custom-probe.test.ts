import type { CustomProbeRequest, ProbePolicy, RequestParameter } from '@/types/domain/sites';

import { mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';

import * as backend from '@/api/call-backend';
import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { buildCustomProbePreview } from '@/views/config/sites/probe-preview';

import { previewBody } from '../../../support/parameter-fields';
import { parameterKeySelector, parameterValueSelector } from '../../../support/parameter-fields';

import ProbePolicyFields from '@/views/config/sites/components/ProbePolicyFields.vue';

const dispose: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
  vi.restoreAllMocks();
});
const request: CustomProbeRequest = {
  version: 2,
  headers: [],
  endpoint: '/v1/images/generations',
  parameters: [
    { enabled: true, key: 'model', value: '', type: 'string', binding: 'model' },
    { enabled: true, key: 'prompt', type: 'string', value: 'ping' },
    { enabled: true, key: 'n', type: 'number', value: '2' },
    { enabled: true, key: 'stream', type: 'boolean', value: 'false' },
    { enabled: true, key: 'metadata.note', type: 'string', value: '' },
    { enabled: true, key: 'options', type: 'json', value: '{"size":128}' },
  ],
};
function mountFields() {
  const site = state.sites[0];
  const policy = reactive<ProbePolicy>({
    version: request.version,
    headers: structuredClone(request.headers),
    parameters: structuredClone(request.parameters),
  });
  function updateEndpoint(value: string | undefined) {
    void wrapper.setProps({ customEndpoint: value });
  }
  const wrapper = mount(ProbePolicyFields, {
    attachTo: document.body,
    props: {
      modelValue: policy,
      protocol: site.protocol,
      modelName: site.models[0].name,
      baseUrl: site.baseUrl,
      probePath: site.connection.probePath,
      customEndpoint: request.endpoint,
      'onUpdate:customEndpoint': updateEndpoint,
    },
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  return { wrapper, policy };
}

describe('模型自定义探针参数与预览', () => {
  it('预览按绑定行替换模型，删除参数后不补充字段', async () => {
    const site = await api.fetchSite('site-1');
    const preview = buildCustomProbePreview(site.baseUrl, site.models[0].name, request);
    expect(preview).toEqual({
      endpoint: 'https://api.openai.com/v1/images/generations',
      body: {
        model: site.models[0].name,
        prompt: 'ping',
        n: 2,
        stream: false,
        metadata: { note: '' },
        options: { size: 128 },
      },
    });
    expect(await api.fetchSite(site.id)).toEqual(site);
    expect(
      buildCustomProbePreview(site.baseUrl, site.models[1].name, {
        ...request,
        parameters: [],
      }),
    ).toEqual({
      endpoint: preview.endpoint,
      body: {},
    });
  });

  it('独立试探使用当前参数草稿，校验端点与凭据，不修改已保存配置和正式探测结果', async () => {
    const site = await api.fetchSite('site-1');
    expect(await api.testCustomProbe(site.id, site.models[0].id, request)).toMatchObject({
      success: true,
      statusCode: 200,
      durationMs: 1250,
      simulated: true,
    });
    expect(await api.fetchSite(site.id)).toEqual(site);
    await expect(
      api.testCustomProbe(site.id, site.models[0].id, {
        ...request,
        version: 2,
        headers: [],
        endpoint: 'https://other.example/probe',
      }),
    ).rejects.toThrow();
    await expect(
      api.testCustomProbe(site.id, site.models[0].id, {
        ...request,
        parameters: [{ enabled: true, key: 'n', value: 'invalid', type: 'number' }],
      }),
    ).rejects.toThrow('数字参数');
    await expect(api.testCustomProbe(site.id, 'missing-model', request)).rejects.toThrow();
    expect(await api.fetchSite(site.id)).toEqual(site);
    state.sites[0].credentialConfigured = false;
    await expect(api.testCustomProbe(site.id, site.models[0].id, request)).rejects.toThrow(
      '访问凭据',
    );
  });

  it('端点原文追加到基址，保留双斜杠且不把完整地址当作覆盖值', async () => {
    state.sites[0].baseUrl = 'https://api.openai.com/v1/';
    const site = state.sites[0];
    for (const [endpoint, resolved] of [
      ['images/generations', 'https://api.openai.com/v1/images/generations'],
      ['/images/generations', 'https://api.openai.com/v1//images/generations'],
      ['https://api.openai.com/custom', 'https://api.openai.com/v1/https://api.openai.com/custom'],
    ]) {
      expect(
        buildCustomProbePreview(site.baseUrl, site.models[0].name, {
          version: 2,
          headers: [],
          endpoint,
          parameters: [],
        }).endpoint,
      ).toBe(resolved);
    }
  });

  it.each([
    ['https://api.openai.com/v1', 'responses'],
    ['https://api.openai.com/v1', '/responses'],
    ['https://api.openai.com/v1/', 'responses'],
    ['https://api.openai.com/v1/', '/responses'],
    ['https://aitogether.site/usage', 'v1/res'],
  ])('基址 %s 与端点 %s 拼接为完整请求地址', (baseUrl, endpoint) => {
    expect(
      buildCustomProbePreview(baseUrl, 'model', {
        version: 2,
        headers: [],
        endpoint,
        parameters: [],
      }).endpoint,
    ).toBe(baseUrl + endpoint);
  });

  it('空端点、无效地址和需要改写的路径同时阻止预览与保存', async () => {
    const site = await api.fetchSite('site-1');
    for (const endpoint of [
      '',
      ' ',
      ' /custom',
      '/custom ',
      'https://other.example/custom',
      'http://api.openai.com/custom',
      'javascript:alert(1)',
      'https://[',
      'https://user:secret@api.openai.com/custom',
      '/cus\ntom',
      '/cus\rtom',
      '/cus\ttom',
      '/../custom',
      '/./custom',
      '/%2e%2e/custom',
      '/cus\\tom',
    ]) {
      const draft = { ...request, endpoint };
      expect(() => buildCustomProbePreview(site.baseUrl, site.models[0].name, draft)).toThrow();
      await expect(api.savePolicy(site.id, site.models[0].id, null, draft)).rejects.toThrow();
      await expect(api.testCustomProbe(site.id, site.models[0].id, draft)).rejects.toThrow();
    }
    expect(await api.fetchSite(site.id)).toEqual(site);
  });

  it('拒绝空字段、重复字段、危险路径和类型错误', async () => {
    const site = await api.fetchSite('site-1');
    const invalidParameters: RequestParameter[][] = [
      [{ enabled: true, key: '', type: 'string', value: '' }],
      [{ enabled: true, key: ' model', type: 'string', value: 'override' }],
      [{ enabled: true, key: 'metadata..note', type: 'string', value: '' }],
      [{ enabled: true, key: '__proto__.polluted', type: 'string', value: 'yes' }],
      [
        {
          enabled: true,
          key: 'metadata.constructor.prototype.polluted',
          type: 'string',
          value: 'yes',
        },
      ],
      [{ enabled: true, key: 'n', type: 'number', value: '' }],
      [{ enabled: true, key: 'stream', type: 'boolean', value: 'yes' }],
      [{ enabled: true, key: 'options', type: 'json', value: '{' }],
      [
        { enabled: true, key: 'n', type: 'number', value: '1' },
        { enabled: true, key: 'n', type: 'number', value: '2' },
      ],
    ];
    for (const parameters of invalidParameters) {
      const draft = { ...request, parameters };
      expect(() => buildCustomProbePreview(site.baseUrl, site.models[0].name, draft)).toThrow();
      await expect(api.savePolicy(site.id, site.models[0].id, null, draft)).rejects.toThrow();
    }
    expect(await api.fetchSite(site.id)).toEqual(site);
    expect(Object.prototype).not.toHaveProperty('polluted');
  });

  it('保存参数列表只修改指定模型，自定义与常规策略互斥；标准协议保留首 Token', async () => {
    const site = await api.fetchSite('site-1');
    const draft = structuredClone(request);
    await expect(api.savePolicy(site.id, site.models[0].id, site.policy, draft)).rejects.toThrow(
      '不能与常规',
    );
    await expect(api.savePolicy(site.id, null, site.policy, draft)).rejects.toThrow('仅用于模型');
    const saved = await api.savePolicy(site.id, site.models[0].id, null, draft);
    expect(saved.models[0].customRequest).toEqual(request);
    expect(saved.models[1]).toEqual(site.models[1]);
    expect(saved.policy).toEqual(site.policy);
    expect(saved.connection).toEqual(site.connection);
    draft.parameters[0].value = 'unsaved';
    expect((await api.fetchSite(site.id)).models[0].customRequest).toEqual(request);
    const probed = await api.runProbe(site.id);
    expect(probed.models[0].firstTokenMs).toBeGreaterThan(0);
    expect(probed.models[0].durationMs).toBeGreaterThan(0);
    expect(probed.models[1].firstTokenMs).toBeGreaterThan(0);
  });

  it('参数校验与预览随输入更新，完整 JSON 仅显示在只读预览', async () => {
    const { wrapper, policy } = mountFields();
    expect(wrapper.find('textarea').exists()).toBe(true);
    expect(wrapper.find('[aria-label="最大输出"]').exists()).toBe(false);
    await vi.waitFor(() =>
      expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"n": 2'),
    );
    await wrapper.get(parameterKeySelector('prompt')).setValue('model');
    await expect(wrapper.vm.validate()).rejects.toThrow('参数字段不能重复');
    expect(wrapper.get('[aria-label="有效请求预览"]').text()).toBe('请先修正配置后查看预览。');
    await wrapper.get('[aria-label="参数字段 2"]').setValue('prompt');
    await wrapper.get(parameterValueSelector('prompt')).setValue('changed');
    await vi.waitFor(() =>
      expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"prompt": "changed"'),
    );
    expect(policy.parameters.find((parameter) => parameter.key === 'prompt')?.value).toBe(
      'changed',
    );
    await wrapper.get('[data-parameter-key="prompt"] button[title="删除参数"]').trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.get('[aria-label="请求体 JSON"]').text()).not.toContain('"prompt"'),
    );
  });

  it('切换自定义和常规预览、修改端点与参数均在本地完成，不调用后端', async () => {
    const calls = vi.spyOn(backend, 'callBackend');
    const before = JSON.stringify(state.sites[0]);
    const { wrapper } = mountFields();
    expect(wrapper.get('[aria-label="请求地址"] pre').text()).toContain(
      'POST https://api.openai.com/v1/images/generations',
    );
    expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"model": "gpt-demo"');
    await wrapper.get('[aria-label="自定义探测端点"]').setValue('/custom');
    await wrapper.get(parameterValueSelector('prompt')).setValue('new prompt');
    expect(wrapper.get('[aria-label="请求地址"] pre').text()).toContain(
      'POST https://api.openai.com/custom',
    );
    expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"prompt": "new prompt"');
    await wrapper.setProps({ customEndpoint: undefined });
    expect(previewBody(wrapper)).toMatchObject({
      model: 'gpt-demo',
      prompt: 'new prompt',
    });
    expect(wrapper.get('[aria-label="请求地址"] pre').text()).toContain(
      'POST https://api.openai.com/chat/completions',
    );
    await wrapper.setProps({ customEndpoint: '/custom', modelName: 'gpt-6-astra' });
    expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"model": "gpt-6-astra"');
    expect(wrapper.get('[aria-label="请求体 JSON"]').text()).toContain('"prompt": "new prompt"');
    expect(calls).not.toHaveBeenCalled();
    expect(JSON.stringify(state.sites[0])).toBe(before);
  });

  it('切换自定义端点共用参数草稿，保留启用状态和编辑内容', async () => {
    const { wrapper, policy } = mountFields();
    const checkbox = wrapper.get<HTMLInputElement>(
      '[data-parameter-key="model"] input[type="checkbox"]',
    );
    await checkbox.setValue(false);
    expect(wrapper.get('[aria-label="请求体 JSON"]').text()).not.toContain('"model"');
    await expect(wrapper.vm.validate()).resolves.toBeUndefined();
    expect(policy.parameters[0].enabled).toBe(false);
    await wrapper.setProps({ customEndpoint: undefined });
    await wrapper
      .get<HTMLInputElement>('[data-parameter-key="n"] input[type="checkbox"]')
      .setValue(false);
    await wrapper.get(parameterValueSelector('prompt')).setValue('切换后修改');
    expect(previewBody(wrapper)).not.toHaveProperty('n');
    await wrapper.setProps({ customEndpoint: request.endpoint });
    expect(
      wrapper.get<HTMLInputElement>('[data-parameter-key="model"] input[type="checkbox"]').element
        .checked,
    ).toBe(false);
    expect(previewBody(wrapper)).toMatchObject({ prompt: '切换后修改' });
    await wrapper.setProps({ customEndpoint: undefined });
    expect(previewBody(wrapper)).not.toHaveProperty('n');
    expect(policy.parameters.find((parameter) => parameter.key === 'n')?.enabled).toBe(false);
  });
});
