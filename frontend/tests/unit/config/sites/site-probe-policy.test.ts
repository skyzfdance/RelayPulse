import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Select } from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';

import * as backend from '@/api/call-backend';
import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteDetailsStore, useSiteStore } from '@/stores/site';

import {
  chatPolicy,
  messagesValue,
  parameterKeySelector,
  parameterValueSelector,
  policyBody,
  policyPrompt,
  policyWithBody,
} from '../../../support/parameter-fields';

import ProbePolicyFields from '@/views/config/sites/components/ProbePolicyFields.vue';
import SiteProbePolicyDrawer from '@/views/config/sites/components/SiteProbePolicyDrawer.vue';

const dispose: (() => void)[] = [];
beforeEach(async () => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
  await useSiteStore().loadSite({ autoLoadDetails: false });
  await useSiteDetailsStore().loadSiteDetails();
});
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
  vi.restoreAllMocks();
});
function mountFields() {
  const policy = reactive(structuredClone(state.sites[0].policy));
  const wrapper = mount(ProbePolicyFields, {
    attachTo: document.body,
    props: { modelValue: policy, protocol: 'openai-compatible' },
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  return { wrapper, policy };
}
async function mountDrawer() {
  const wrapper = mount(SiteProbePolicyDrawer, {
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  await flushPromises();
  const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
  const button = (label: string) =>
    drawer.findAll('button').find((button) => button.text().replace(/\s/gu, '') === label)!;
  return { wrapper, drawer, button };
}

describe('探针请求参数编辑', () => {
  it('基础参数统一进入表格，数字仅校验类型，预览不限制协议取值', async () => {
    const { wrapper, policy } = mountFields();
    expect(wrapper.find('[aria-label="最大输出"]').exists()).toBe(false);
    expect(wrapper.find('[aria-label="探针提示词"]').exists()).toBe(false);
    expect(
      wrapper.findAll('[data-parameter-key]').map((row) => row.attributes('data-parameter-key')),
    ).toEqual(['model', 'messages', 'stream', 'max_tokens', 'reasoning_effort']);
    const input = wrapper.get<HTMLInputElement>(parameterValueSelector('max_tokens'));
    for (const value of ['', 'abc']) {
      await input.setValue(value);
      expect(wrapper.text()).toContain('数字参数必须为有效数字');
      await expect(wrapper.vm.validate()).rejects.toBeDefined();
      expect(input.element.value).toBe(value);
    }
    for (const value of ['0', '129', '1.5']) {
      await input.setValue(value);
      expect(JSON.parse(wrapper.get('pre').text()).max_tokens).toBe(Number(value));
      expect(wrapper.text()).not.toContain('数字参数必须为有效数字');
    }
    expect(wrapper.vm.canSubmit).toBe(true);
    expect(policyBody(policy).max_tokens).toBe(1.5);
    await input.setValue('32');
    await expect(wrapper.vm.validate()).resolves.toBeUndefined();
    expect(policyBody(policy).max_tokens).toBe(32);
    await input.setValue('8');
    expect(policy).toEqual(state.sites[0].policy);
  });

  it('切换到另一份策略会重新载入参数，不沿用之前的草稿', async () => {
    const { wrapper } = mountFields();
    await wrapper.get(parameterValueSelector('max_tokens')).setValue('32');
    await wrapper.setProps({
      modelValue: reactive(chatPolicy('inherited prompt')),
      disabled: true,
    });
    expect(wrapper.get<HTMLInputElement>(parameterValueSelector('max_tokens')).element.value).toBe(
      '8',
    );
    expect(JSON.parse(wrapper.get('pre').text())).toMatchObject({ max_tokens: 8 });
  });

  it('新增对象路径自动补父级，校验类型值；删除后不重新注入参数', async () => {
    const { wrapper } = mountFields();
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('添加参数'))!
      .trigger('click');
    expect(wrapper.text()).toContain('请输入参数字段');
    await expect(wrapper.vm.validate()).rejects.toBeDefined();
    await wrapper.get(parameterKeySelector('')).setValue('provider.option');
    await wrapper.get(parameterValueSelector('provider.option')).setValue('high');
    expect(JSON.parse(wrapper.get('pre').text())).toMatchObject({ provider: { option: 'high' } });
    expect(wrapper.find('[data-parameter-key="provider"]').exists()).toBe(true);
    wrapper.findAllComponents(Select).slice(-1)[0].vm.$emit('update:value', 'number');
    await flushPromises();
    expect(wrapper.text()).toContain('数字参数必须为有效数字');
    await expect(wrapper.vm.validate()).rejects.toBeDefined();
    await wrapper
      .get('[data-parameter-key="provider"] button[title="删除参数及其子参数"]')
      .trigger('click');
    expect(JSON.parse(wrapper.get('pre').text())).not.toHaveProperty('provider');
    await wrapper
      .get('[data-parameter-key="reasoning_effort"] button[title="删除参数"]')
      .trigger('click');
    expect(JSON.parse(wrapper.get('pre').text())).not.toHaveProperty('reasoning_effort');
  });

  it('父级停用保留子级开关，恢复后仍按原子级配置发送；支持空对象', async () => {
    const { wrapper } = mountFields();
    await wrapper.setProps({
      protocol: 'openai-responses',
      modelValue: reactive(
        policyWithBody({
          model: '',
          input: '只回复 OK',
          stream: true,
          max_output_tokens: 8,
          reasoning: { effort: 'low' },
        }),
      ),
    });
    const parent = wrapper.get<HTMLInputElement>(
      '[data-parameter-key="reasoning"] input[type="checkbox"]',
    );
    const child = wrapper.get<HTMLInputElement>(
      '[data-parameter-key="reasoning.effort"] input[type="checkbox"]',
    );
    await parent.setValue(false);
    expect(JSON.parse(wrapper.get('pre').text())).not.toHaveProperty('reasoning');
    expect(child.element.checked).toBe(true);
    expect(child.element.disabled).toBe(true);
    expect(
      wrapper.get<HTMLInputElement>(parameterValueSelector('reasoning.effort')).element.value,
    ).toBe('low');
    await parent.setValue(true);
    expect(JSON.parse(wrapper.get('pre').text()).reasoning).toEqual({ effort: 'low' });
    await child.setValue(false);
    expect(JSON.parse(wrapper.get('pre').text()).reasoning).toEqual({});
    await parent.setValue(false);
    await parent.setValue(true);
    expect(child.element.checked).toBe(false);
    expect(JSON.parse(wrapper.get('pre').text()).reasoning).toEqual({});
  });

  it('重新输入父级字段时同步保留子级路径和值', async () => {
    const { wrapper } = mountFields();
    await wrapper.setProps({
      protocol: 'openai-responses',
      modelValue: reactive(
        policyWithBody({
          model: '',
          input: '只回复 OK',
          stream: true,
          max_output_tokens: 8,
          reasoning: { effort: 'low' },
        }),
      ),
    });
    const input = wrapper.get(parameterKeySelector('reasoning'));
    await input.setValue('');
    await input.setValue('options');
    const body = JSON.parse(wrapper.get('pre').text());
    expect(body).not.toHaveProperty('reasoning');
    expect(body.options).toEqual({ effort: 'low' });
    expect(wrapper.find('[data-parameter-key="options.effort"]').exists()).toBe(true);
  });

  it('所有参数都可停用，保存为空请求并保留禁用项的原值', async () => {
    const calls = vi.spyOn(backend, 'callBackend');
    const { wrapper, drawer, button } = await mountDrawer();
    await drawer.get(parameterValueSelector('max_tokens')).setValue('invalid');
    for (const checkbox of drawer.findAll<HTMLInputElement>('input[type="checkbox"]')) {
      if (!checkbox.element.disabled) await checkbox.setValue(false);
    }
    expect(JSON.parse(drawer.get('pre').text())).toEqual({});
    expect(drawer.text()).not.toContain('数字参数必须为有效数字');
    expect(button('保存').attributes('disabled')).toBeUndefined();
    await button('保存').trigger('click');
    await flushPromises();
    expect(calls).toHaveBeenCalledOnce();
    expect(wrapper.emitted('close')).toHaveLength(1);
    const saved = (await api.fetchSite('site-1')).policy;
    expect(policyBody(saved)).toEqual({});
    expect(saved.parameters.every((parameter) => !parameter.enabled)).toBe(true);
    expect(saved.parameters.find((parameter) => parameter.key === 'max_tokens')?.value).toBe(
      'invalid',
    );
  });

  it('默认策略抽屉打开及编辑即时生成本地预览，不调用后端或保存草稿', async () => {
    const calls = vi.spyOn(backend, 'callBackend');
    const before = JSON.stringify(state.sites[0]);
    const { drawer } = await mountDrawer();
    expect(JSON.parse(drawer.get('pre').text())).toMatchObject({
      model: '<模型名称>',
      messages: [{ role: 'user', content: '只回复 OK' }],
      max_tokens: 8,
    });
    await drawer.get(parameterValueSelector('messages')).setValue(messagesValue('new prompt'));
    expect(JSON.parse(drawer.get('pre').text()).messages).toEqual([
      { role: 'user', content: 'new prompt' },
    ]);
    await drawer.get(parameterValueSelector('max_tokens')).setValue('24');
    expect(JSON.parse(drawer.get('pre').text()).max_tokens).toBe(24);
    await drawer.get(parameterValueSelector('messages')).setValue('{');
    expect(drawer.get('pre').text()).toBe('请先修正配置后查看预览。');
    expect(calls).not.toHaveBeenCalled();
    expect(JSON.stringify(state.sites[0])).toBe(before);
  });

  it('模型绑定随名称更新，协议变化不重写使用者的参数', async () => {
    const calls = vi.spyOn(backend, 'callBackend');
    const { wrapper } = mountFields();
    await wrapper.setProps({ modelName: 'gpt-6-astra', protocol: 'openai-responses' });
    expect(JSON.parse(wrapper.get('pre').text())).toEqual({
      model: 'gpt-6-astra',
      messages: [{ role: 'user', content: '只回复 OK' }],
      stream: true,
      max_tokens: 8,
      reasoning_effort: 'low',
    });
    expect(calls).not.toHaveBeenCalled();
  });

  it('有效草稿正常保存；失败保留输入，成功回填当前站点', async () => {
    const save = vi.spyOn(api, 'savePolicy');
    const { wrapper, drawer, button } = await mountDrawer();
    await drawer.get(parameterValueSelector('messages')).setValue('{');
    await button('保存').trigger('click');
    await flushPromises();
    expect(save).not.toHaveBeenCalled();
    expect(wrapper.emitted('close')).toBeUndefined();
    await drawer.get(parameterValueSelector('messages')).setValue(messagesValue('saved prompt'));
    window.history.replaceState({}, '', '/?scenario=action-error');
    await button('保存').trigger('click');
    await flushPromises();
    expect(wrapper.emitted('close')).toBeUndefined();
    expect(drawer.get<HTMLTextAreaElement>(parameterValueSelector('messages')).element.value).toBe(
      messagesValue('saved prompt'),
    );
    expect(policyPrompt(useSiteDetailsStore().selectedSiteDetail?.policy)).toBe('只回复 OK');
    window.history.replaceState({}, '', '/');
    await button('保存').trigger('click');
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(policyPrompt(useSiteDetailsStore().selectedSiteDetail?.policy)).toBe('saved prompt');
    expect(save).toHaveBeenLastCalledWith(
      'site-1',
      null,
      expect.objectContaining({
        version: 2,
        parameters: expect.arrayContaining([
          expect.objectContaining({
            key: 'messages',
            value: messagesValue('saved prompt'),
            enabled: true,
          }),
        ]),
      }),
      null,
    );
  });

  it('截图中的自由参数可保存，重新打开保留字段和值及父子勾选状态', async () => {
    const { wrapper, drawer, button } = await mountDrawer();
    await drawer.get(parameterKeySelector('messages')).setValue('input');
    wrapper
      .getComponent(ProbePolicyFields)
      .findAllComponents(Select)[1]
      .vm.$emit('update:value', 'string');
    await flushPromises();
    await drawer.get(parameterValueSelector('input')).setValue('只回复 OK');
    await drawer.get(parameterKeySelector('reasoning_effort')).setValue('reasoning.effort');
    expect(button('保存').attributes('disabled')).toBeUndefined();
    expect(JSON.parse(drawer.get('pre').text())).toEqual({
      model: '<模型名称>',
      input: '只回复 OK',
      stream: true,
      max_tokens: 8,
      reasoning: { effort: 'low' },
    });
    await drawer
      .get<HTMLInputElement>('[data-parameter-key="reasoning"] input[type="checkbox"]')
      .setValue(false);
    expect(button('保存').attributes('disabled')).toBeUndefined();
    await button('保存').trigger('click');
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    const saved = (await api.fetchSite('site-1')).policy;
    expect(saved.parameters.find((parameter) => parameter.key === 'reasoning')?.enabled).toBe(
      false,
    );
    expect(
      saved.parameters.find((parameter) => parameter.key === 'reasoning.effort'),
    ).toMatchObject({ enabled: true, value: 'low' });
    expect(policyBody(saved)).toEqual({
      model: '<模型名称>',
      input: '只回复 OK',
      stream: true,
      max_tokens: 8,
    });
    wrapper.unmount();
    const reopened = await mountDrawer();
    expect(
      reopened.drawer.get<HTMLInputElement>(parameterValueSelector('input')).element.value,
    ).toBe('只回复 OK');
    const child = reopened.drawer.get<HTMLInputElement>(
      '[data-parameter-key="reasoning.effort"] input[type="checkbox"]',
    );
    expect(child.element.checked).toBe(true);
    expect(child.element.disabled).toBe(true);
    await reopened.drawer
      .get<HTMLInputElement>('[data-parameter-key="reasoning"] input[type="checkbox"]')
      .setValue(true);
    expect(JSON.parse(reopened.drawer.get('pre').text()).reasoning).toEqual({ effort: 'low' });
    await child.setValue(false);
    expect(JSON.parse(reopened.drawer.get('pre').text()).reasoning).toEqual({});
    expect(reopened.button('保存').attributes('disabled')).toBeUndefined();
  });

  it('保存期间阻止重复提交，卸载后返回的结果不回填另一站点', async () => {
    let resolve!: (site: Awaited<ReturnType<typeof api.fetchSite>>) => void;
    const saved = await api.fetchSite('site-1');
    const save = vi.spyOn(api, 'savePolicy').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const { wrapper, button } = await mountDrawer();
    await button('保存').trigger('click');
    await flushPromises();
    expect(save).toHaveBeenCalledOnce();
    expect(button('保存').attributes('disabled')).toBeDefined();
    expect(button('取消').attributes('disabled')).toBeDefined();
    await button('保存').trigger('click');
    expect(save).toHaveBeenCalledOnce();
    wrapper.unmount();
    useSiteStore().selectedSiteId = 'site-2';
    await useSiteDetailsStore().loadSiteDetails();
    resolve(saved);
    await flushPromises();
    expect(useSiteDetailsStore().selectedSiteDetail?.id).toBe('site-2');
    expect(wrapper.emitted('close')).toBeUndefined();
  });
});
