import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Select } from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import {
  chatPolicy,
  messagesValue,
  parameterKeySelector,
  parameterValueSelector,
  policyPrompt,
  setPolicyPrompt,
} from '../../../support/parameter-fields';

import AddModelModal from '@/views/config/sites/components/AddModelModal.vue';
import AddSiteModal from '@/views/config/sites/components/AddSiteModal.vue';
import AdvancedSettingsDrawer from '@/views/config/sites/components/AdvancedSettingsDrawer.vue';
import ModelProbeSettingsDrawer from '@/views/config/sites/components/ModelProbeSettingsDrawer.vue';
import ModelSyncModal from '@/views/config/sites/components/ModelSyncModal.vue';
import SiteDetail from '@/views/config/sites/components/SiteDetail.vue';
import SiteList from '@/views/config/sites/components/SiteList.vue';
import SiteProbePolicyDrawer from '@/views/config/sites/components/SiteProbePolicyDrawer.vue';
import SitesView from '@/views/config/sites/SitesView.vue';

let dispose: (() => void) | undefined;
beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  dispose?.();
  vi.restoreAllMocks();
});

async function mountPage() {
  const wrapper = mount(SitesView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  dispose = () => wrapper.unmount();
  await vi.waitFor(() => expect(wrapper.find('input#name').exists()).toBe(true));
  return { wrapper, store: useSiteStore(), details: useSiteDetailsStore() };
}
function button(wrapper: Awaited<ReturnType<typeof mountPage>>['wrapper'], label: string) {
  return wrapper
    .findAll('button')
    .find((item) => (item.attributes('aria-label') ?? item.text().replace(/\s/gu, '')) === label)!;
}
function overlayButton(container: Element, label: string) {
  return new DOMWrapper(
    Array.from(container.querySelectorAll('button')).find(
      (item) => item.textContent?.replace(/\s/gu, '') === label,
    )!,
  );
}

describe('站点详情与页面共享数据', () => {
  it('详情确认删除后由 store 刷新列表、选中项和详情', async () => {
    const { wrapper, store, details } = await mountPage();
    await wrapper.get('[data-sort-id="site-2"]').trigger('click');
    await flushPromises();
    const remove = vi.spyOn(store, 'removeSite');
    await button(wrapper, '删除站点').trigger('click');
    await vi.waitFor(() => expect(document.querySelector('.ant-popconfirm')).not.toBeNull());
    await overlayButton(document.querySelector('.ant-popconfirm')!, '删除').trigger('click');
    await flushPromises();

    expect(remove).toHaveBeenCalledExactlyOnceWith();
    expect(store.sites.map((site) => site.id)).toEqual(['site-1', 'site-3']);
    expect(details.selectedSiteDetail?.id).toBe('site-1');
    expect(wrapper.getComponent(SiteDetail).find('h2').text()).toBe('OpenAI 主站');
  });

  it('实际页面不依赖 provider，详情可直接初始化', async () => {
    const warn = vi.spyOn(console, 'warn');
    const { wrapper, details } = await mountPage();
    expect(wrapper.getComponent(SiteDetail).find('h2').text()).toBe('OpenAI 主站');
    expect(details.selectedSiteDetail?.id).toBe('site-1');
    expect(warn.mock.calls.flat().some((value) => String(value).includes('[Vue warn]'))).toBe(
      false,
    );
  });

  it('点击列表中的站点开关只改变小面板显示设置，不切换当前详情', async () => {
    const { wrapper, store } = await mountPage();
    const currentId = store.selectedSiteId!;
    const other = store.sites[1];
    await wrapper
      .get(`[role="switch"][aria-label="在小面板中隐藏 ${other.name}"]`)
      .trigger('click');
    await flushPromises();
    expect(store.selectedSiteId!).toBe(currentId);
    expect(store.sites.find((site) => site.id === other.id)?.enabled).toBe(false);
  });
  it('切换当前站点的小面板显示设置，不影响健康状态或批量探测', async () => {
    const { wrapper, store, details } = await mountPage();
    const siteId = store.selectedSiteId!;
    const summary = { ...store.sites.find((site) => site.id === siteId)! };
    const excludedModel = { ...details.selectedSiteDetail!.models[2] };
    for (const enabled of [false, true]) {
      await wrapper.get(`[data-sort-id="${siteId}"] [role="switch"]`).trigger('click');
      await flushPromises();
      expect(store.selectedSiteId!).toBe(siteId);
      expect(store.sites.find((site) => site.id === siteId)).toEqual({ ...summary, enabled });
      expect(details.selectedSiteDetail?.enabled).toBe(enabled);
      expect(wrapper.getComponent(SiteDetail).text()).toContain(
        enabled ? '小面板显示' : '小面板隐藏',
      );
      const probe = button(wrapper, '探测已启用模型');
      const probeModel = button(wrapper, '立即探测模型 gpt-demo');
      expect(probe.attributes('disabled')).toBeUndefined();
      expect(probeModel.attributes('disabled')).toBeUndefined();
      await probe.trigger('click');
      await flushPromises();
      expect(details.selectedSiteDetail?.models[0].durationMs).toBe(1250);
      expect(details.selectedSiteDetail?.models[2]).toEqual(excludedModel);
      expect(details.selectedSiteDetail?.enabled).toBe(enabled);
    }
  });
  it.each([true, false])(
    '小面板显示设置为 %s 时，没有启用模型只禁用批量探测，仍可手动探测单个模型',
    async (enabled) => {
      state.sites[0].enabled = enabled;
      for (const model of state.sites[0].models) model.enabled = false;
      const { wrapper, details } = await mountPage();
      expect(button(wrapper, '探测已启用模型').attributes('disabled')).toBeDefined();
      const probeModel = button(wrapper, '立即探测模型 legacy-demo');
      expect(probeModel.attributes('disabled')).toBeUndefined();
      await probeModel.trigger('click');
      await flushPromises();
      expect(details.selectedSiteDetail?.models[2]).toMatchObject({
        enabled: false,
        health: 'healthy',
        firstTokenMs: 420,
        durationMs: 1250,
      });
      expect(details.selectedSiteDetail?.enabled).toBe(enabled);
      expect(button(wrapper, '探测已启用模型').attributes('disabled')).toBeDefined();
    },
  );
  it('保存失败保留草稿；成功后列表与详情同步更新并清除凭据输入', async () => {
    const { wrapper, details } = await mountPage();
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
    await wrapper.find('input#name').setValue('编辑后的站点');
    await wrapper.find('input[type="password"]').setValue('test-secret');
    window.history.replaceState({}, '', '/?scenario=action-error');
    await button(wrapper, '保存变更').trigger('click');
    await flushPromises();
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeUndefined();
    expect(details.selectedSiteDetail?.name).toBe('OpenAI 主站');
    expect(wrapper.find<HTMLInputElement>('input#name').element.value).toBe('编辑后的站点');
    window.history.replaceState({}, '', '/');
    await button(wrapper, '保存变更').trigger('click');
    await flushPromises();
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
    expect(wrapper.getComponent(SiteList).text()).toContain('编辑后的站点');
    expect(wrapper.getComponent(SiteDetail).find('h2').text()).toBe('编辑后的站点');
    expect(wrapper.find<HTMLInputElement>('input[type="password"]').element.value).toBe('');
    expect((await api.fetchSites())[0].name).toBe('编辑后的站点');
  });
  it('刷新站点数据不覆盖正在编辑的草稿，改回原值后禁用保存', async () => {
    const { wrapper, store, details } = await mountPage();
    await wrapper.find('input#name').setValue('草稿');
    await api.runProbe(store.selectedSiteId!);
    await details.loadSiteDetails();
    expect(wrapper.find<HTMLInputElement>('input#name').element.value).toBe('草稿');
    expect(details.selectedSiteDetail?.models[0].durationMs).toBe(1250);
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeUndefined();
    await wrapper.get('input#name').setValue('OpenAI 主站');
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
  });
  it('无效输入不提交，草稿未保存时禁止模型操作', async () => {
    const { wrapper, details } = await mountPage();
    await wrapper.find('input#baseUrl').setValue('invalid');
    await button(wrapper, '保存变更').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('请输入有效的 Base URL');
    expect(details.selectedSiteDetail?.baseUrl).toBe('https://api.openai.com');
    expect(button(wrapper, '同步模型').attributes('disabled')).toBeDefined();
  });
  it('独立添加模型弹窗提交后，详情列表与左侧模型数量一起更新', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '添加模型').trigger('click');
    await vi.waitFor(() =>
      expect(document.querySelector('.ant-modal input[aria-label="模型名称"]')).not.toBeNull(),
    );
    const modal = document.querySelector('.ant-modal')!;
    await new DOMWrapper(modal.querySelector('input')!).setValue('new-demo-model');
    await overlayButton(modal, '添加').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(AddModelModal).exists()).toBe(false);
    expect(details.selectedSiteDetail?.models).toHaveLength(4);
    expect(wrapper.getComponent(SiteList).text()).toContain('4 个模型');
    expect(wrapper.getComponent(SiteDetail).text()).toContain('new-demo-model');
  });
  it.each([true, false])('添加站点连接并加载模型，保留小面板显示选项 %s', async (enabled) => {
    const { wrapper, store, details } = await mountPage();
    await button(wrapper, '添加站点').trigger('click');
    await vi.waitFor(() => expect(document.querySelector('.ant-modal input#name')).not.toBeNull());
    const modal = document.querySelector('.ant-modal')!;
    await new DOMWrapper(modal.querySelector('input#name')!).setValue('新站点');
    await new DOMWrapper(modal.querySelector('input#baseUrl')!).setValue('https://new.example.com');
    const checkbox = new DOMWrapper(
      modal.querySelector<HTMLInputElement>('input[type="checkbox"]')!,
    );
    expect(checkbox.element.checked).toBe(true);
    expect(modal.textContent).not.toContain('尚未配置凭据');
    await overlayButton(modal, '连接并获取模型').trigger('click');
    await flushPromises();
    expect(modal.textContent).toContain('请输入访问凭据');
    expect(store.sites).toHaveLength(3);
    await new DOMWrapper(modal.querySelector('input[type="password"]')!).setValue(
      'test-only-secret',
    );
    if (!enabled) await checkbox.setValue(false);
    window.history.replaceState({}, '', '/?scenario=action-error');
    await overlayButton(modal, '连接并获取模型').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(AddSiteModal).exists()).toBe(true);
    expect(store.sites).toHaveLength(3);
    expect(checkbox.element.checked).toBe(enabled);
    window.history.replaceState({}, '', '/');
    await overlayButton(modal, '连接并获取模型').trigger('click');
    await flushPromises();
    expect(store.sites).toHaveLength(4);
    expect(wrapper.findComponent(AddSiteModal).exists()).toBe(false);
    expect(details.selectedSiteDetail).toMatchObject({
      name: '新站点',
      enabled,
      credentialConfigured: true,
    });
    expect(details.selectedSiteDetail?.models).toHaveLength(2);
    expect(JSON.stringify(await api.fetchSites())).not.toContain('test-only-secret');
    expect(wrapper.getComponent(SiteList).text()).toContain('新站点');
    expect(wrapper.getComponent(SiteDetail).find('h2').text()).toBe('新站点');
  });
  it('详情展示接口返回的连接和探针配置，编辑请求头不直接修改已保存数据', async () => {
    state.sites[0].connection.intervalSeconds = 17;
    state.sites[0].connection.headers = [{ key: 'X-Route', value: 'server-route' }];
    state.sites[0].policy = chatPolicy('接口返回的探针提示词', 37);
    const { wrapper } = await mountPage();
    const detail = wrapper.getComponent(SiteDetail);
    expect(detail.text()).toContain('已启用 5 / 5 个请求参数');

    await button(wrapper, '高级设置').trigger('click');
    await vi.waitFor(() =>
      expect(document.querySelector('.ant-drawer input[role="spinbutton"]')).not.toBeNull(),
    );
    const drawer = document.querySelector('.ant-drawer')!;
    expect(drawer.querySelector<HTMLInputElement>('input[role="spinbutton"]')!.value).toBe('17');
    const header = new DOMWrapper(
      drawer.querySelector<HTMLInputElement>('[aria-label="请求头值 1"]')!,
    );
    expect(header.element.value).toBe('server-route');
    await header.setValue('unsaved-route');
    expect((await api.fetchSite('site-1')).connection.headers[0].value).toBe('server-route');
  });

  it('高级设置独立保存并更新当前站点，不产生基本信息草稿', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '高级设置').trigger('click');
    await vi.waitFor(() =>
      expect(document.querySelector('.ant-drawer input[role="spinbutton"]')).not.toBeNull(),
    );
    const drawer = document.querySelector('.ant-drawer')!;
    const interval = new DOMWrapper(
      drawer.querySelector<HTMLInputElement>('input[role="spinbutton"]')!,
    );
    await interval.setValue('15');
    await interval.trigger('blur');
    expect(interval.element.value).toBe('15');
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
    await overlayButton(drawer, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(false);
    expect(details.selectedSiteDetail?.connection.intervalSeconds).toBe(15);
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
  });
  it.each(['取消', '关闭按钮', 'Escape'])(
    '高级设置通过 %s 关闭后丢弃草稿，重开读取当前站点已保存配置',
    async (closeWith) => {
      state.sites[0].connection.headers = [{ key: 'X-Route', value: 'saved' }];
      const { wrapper, details } = await mountPage();
      await button(wrapper, '高级设置').trigger('click');
      await flushPromises();
      const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
      await drawer.get('input#intervalSeconds').setValue('15');
      await drawer.get('[aria-label="请求头值 1"]').setValue('unsaved');
      expect(details.selectedSiteDetail?.connection.intervalSeconds).toBe(300);
      expect(details.selectedSiteDetail?.connection.headers[0].value).toBe('saved');
      expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
      if (closeWith === '取消') await overlayButton(drawer.element, '取消').trigger('click');
      else if (closeWith === '关闭按钮') await drawer.get('.ant-drawer-close').trigger('click');
      else await drawer.get('[role="dialog"]').trigger('keydown', { key: 'Escape', keyCode: 27 });
      await flushPromises();
      expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(false);
      const updated = await api.fetchSite('site-1');
      updated.connection.intervalSeconds = 45;
      updated.connection.headers[0].value = 'current';
      details.setSiteDetail(updated);
      await button(wrapper, '高级设置').trigger('click');
      await flushPromises();
      const reopened = new DOMWrapper(document.querySelector('.ant-drawer')!);
      expect(reopened.get<HTMLInputElement>('input#intervalSeconds').element.value).toBe('45');
      expect(reopened.get<HTMLInputElement>('[aria-label="请求头值 1"]').element.value).toBe(
        'current',
      );
    },
  );
  it('高级设置保存不会提交或丢弃基本信息草稿，随后保存基本信息保留高级设置', async () => {
    const { wrapper, details } = await mountPage();
    await wrapper.get('input#name').setValue('未保存名称');
    await wrapper.get('input#baseUrl').setValue('invalid');
    await wrapper.get('input[type="password"]').setValue('test-only-secret');
    await button(wrapper, '高级设置').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('input#intervalSeconds').setValue('15');
    await overlayButton(drawer.element, '保存').trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(false),
    );
    expect(details.selectedSiteDetail).toMatchObject({
      name: 'OpenAI 主站',
      baseUrl: 'https://api.openai.com',
      connection: { intervalSeconds: 15 },
    });
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('未保存名称');
    expect(wrapper.get<HTMLInputElement>('input#baseUrl').element.value).toBe('invalid');
    expect(wrapper.get<HTMLInputElement>('input[type="password"]').element.value).toBe(
      'test-only-secret',
    );
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeUndefined();
    await wrapper.get('input#baseUrl').setValue('https://new.example.com');
    await button(wrapper, '保存变更').trigger('click');
    await flushPromises();
    expect(await api.fetchSite('site-1')).toMatchObject({
      name: '未保存名称',
      baseUrl: 'https://new.example.com',
      connection: { intervalSeconds: 15 },
    });
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeDefined();
  });
  it('高级设置保存失败留在抽屉且保留输入，重试成功后重开使用保存结果', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '高级设置').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('input#intervalSeconds').setValue('15');
    window.history.replaceState({}, '', '/?scenario=action-error');
    await overlayButton(drawer.element, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(true);
    expect(drawer.get<HTMLInputElement>('input#intervalSeconds').element.value).toBe('15');
    expect(details.selectedSiteDetail?.connection.intervalSeconds).toBe(300);
    window.history.replaceState({}, '', '/');
    await overlayButton(drawer.element, '保存').trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(false),
    );
    await button(wrapper, '高级设置').trigger('click');
    await flushPromises();
    expect(
      document.querySelector<HTMLInputElement>('.ant-drawer input#intervalSeconds')!.value,
    ).toBe('15');
  });
  it('默认策略未保存就关闭会丢弃草稿，重开使用当前站点已保存的策略', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '编辑默认值').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get(parameterValueSelector('messages')).setValue(messagesValue('unsaved prompt'));
    await drawer.get(parameterValueSelector('reasoning_effort')).setValue('high');
    expect(policyPrompt(details.selectedSiteDetail?.policy)).toBe('只回复 OK');
    expect(
      details.selectedSiteDetail?.policy.parameters.find(
        (parameter) => parameter.key === 'reasoning_effort',
      )?.value,
    ).toBe('low');
    await overlayButton(drawer.element, '取消').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(SiteProbePolicyDrawer).exists()).toBe(false);
    const saved = await api.savePolicy(
      'site-1',
      null,
      chatPolicy('current saved prompt', 24, { reasoning_effort: 'medium' }),
      null,
    );
    details.setSiteDetail(saved);
    await button(wrapper, '编辑默认值').trigger('click');
    await flushPromises();
    const reopened = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(reopened.get<HTMLInputElement>(parameterValueSelector('messages')).element.value).toBe(
      messagesValue('current saved prompt'),
    );
    expect(reopened.get<HTMLInputElement>(parameterValueSelector('max_tokens')).element.value).toBe(
      '24',
    );
    expect(
      reopened.get<HTMLInputElement>(parameterValueSelector('reasoning_effort')).element.value,
    ).toBe('medium');
  });
  it('基本信息有草稿时仍可独立保存默认策略，两处保存互不覆盖', async () => {
    const { wrapper, details } = await mountPage();
    await wrapper.get('input#name').setValue('基本信息草稿');
    expect(button(wrapper, '编辑默认值').attributes('disabled')).toBeUndefined();
    await button(wrapper, '编辑默认值').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer
      .get(parameterValueSelector('messages'))
      .setValue(messagesValue('new saved prompt'));
    await overlayButton(drawer.element, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(SiteProbePolicyDrawer).exists()).toBe(false);
    expect(details.selectedSiteDetail?.name).toBe('OpenAI 主站');
    expect(policyPrompt(details.selectedSiteDetail?.policy)).toBe('new saved prompt');
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('基本信息草稿');
    expect(button(wrapper, '保存变更').attributes('disabled')).toBeUndefined();
    await button(wrapper, '保存变更').trigger('click');
    await flushPromises();
    expect(policyPrompt((await api.fetchSite('site-1')).policy)).toBe('new saved prompt');
    expect(details.selectedSiteDetail?.name).toBe('基本信息草稿');
  });
  it('默认策略抽屉保存后更新详情，模型策略抽屉只修改目标模型', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '编辑默认值').trigger('click');
    await vi.waitFor(() =>
      expect(document.querySelector(parameterValueSelector('messages'))).not.toBeNull(),
    );
    await new DOMWrapper(document.querySelector(parameterValueSelector('messages'))!).setValue(
      messagesValue('站点默认提示词'),
    );
    await overlayButton(document.querySelector('.ant-drawer')!, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(SiteProbePolicyDrawer).exists()).toBe(false);
    expect(policyPrompt(details.selectedSiteDetail?.policy)).toBe('站点默认提示词');

    await button(wrapper, `设置模型 ${details.selectedSiteDetail!.models[0].name}`).trigger(
      'click',
    );
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(drawer.findAll('[role="switch"]')).toHaveLength(1);
    expect(drawer.get('[aria-label="使用自定义探针请求"]').attributes('aria-checked')).toBe(
      'false',
    );
    expect(drawer.find('.ant-segmented').exists()).toBe(false);
    expect(drawer.find('[aria-label="配置方式"]').exists()).toBe(false);
    expect(drawer.find('textarea').exists()).toBe(true);
    expect(drawer.get<HTMLInputElement>(parameterValueSelector('messages')).element.value).toBe(
      messagesValue('站点默认提示词'),
    );
    await new DOMWrapper(document.querySelector(parameterValueSelector('messages'))!).setValue(
      messagesValue('模型独立提示词'),
    );
    await overlayButton(document.querySelector('.ant-drawer')!, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
    expect(policyPrompt(details.selectedSiteDetail?.models[0].policy)).toBe('模型独立提示词');
    expect(details.selectedSiteDetail?.models[0].customRequest).toBeNull();
    expect(policyPrompt(details.selectedSiteDetail?.policy)).toBe('站点默认提示词');
    expect(wrapper.getComponent(SiteDetail).text()).toContain('独立策略');
  });
  it.each([false, true])(
    '参数与站点默认相同时保存仍为独立策略，后续默认值变更不覆盖模型（修改后还原：%s）',
    async (editAndRestore) => {
      const { wrapper, details } = await mountPage();
      const site = await api.fetchSite('site-1');
      const model = site.models[0];
      const label = `设置模型 ${model.name}`;
      expect(model.policy).toBeNull();
      expect(wrapper.get(`[data-row-key="${model.id}"]`).text()).toContain('继承站点默认');
      await button(wrapper, label).trigger('click');
      await flushPromises();
      const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
      if (editAndRestore) {
        await drawer.get(parameterValueSelector('messages')).setValue(messagesValue('临时修改'));
        await drawer
          .get(parameterValueSelector('messages'))
          .setValue(messagesValue(policyPrompt(site.policy)!));
      }
      await overlayButton(drawer.element, '保存').trigger('click');
      await flushPromises();
      expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
      expect(details.selectedSiteDetail?.models[0].policy).toEqual(site.policy);
      expect((await api.fetchSite(site.id)).models[0].policy).toEqual(site.policy);
      expect(wrapper.get(`[data-row-key="${model.id}"]`).text()).toContain('独立策略');

      const nextPolicy = JSON.parse(JSON.stringify(site.policy));
      setPolicyPrompt(nextPolicy, '更新后的站点默认策略');
      const current = await api.savePolicy(site.id, null, nextPolicy, null);
      details.setSiteDetail(current);
      await flushPromises();
      expect(current.models[0].policy).toEqual(site.policy);
      expect(current.models[1].policy).toBeNull();
      await button(wrapper, label).trigger('click');
      await flushPromises();
      const reopened = new DOMWrapper(document.querySelector('.ant-drawer')!);
      expect(reopened.get<HTMLInputElement>(parameterValueSelector('messages')).element.value).toBe(
        messagesValue(policyPrompt(site.policy)!),
      );
    },
  );
  it('自定义探针使用端点和参数表，抽屉内部保存后回填目标模型并按接口数据重新打开', async () => {
    state.sites[0].models[0].policy = chatPolicy('已有模型策略');
    const { wrapper, details } = await mountPage();
    const site = await api.fetchSite('site-1');
    const model = site.models[0];
    const save = vi.spyOn(api, 'savePolicy');
    await button(wrapper, `设置模型 ${model.name}`).trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
    expect(drawer.find(parameterValueSelector('max_tokens')).exists()).toBe(false);
    expect(drawer.find(parameterValueSelector('messages')).exists()).toBe(false);
    expect(drawer.find('textarea').exists()).toBe(false);
    expect(drawer.text()).toContain('请求参数');
    expect(drawer.get('[data-parameter-key="model"]').text()).toContain(model.name);
    expect(drawer.get<HTMLInputElement>('[aria-label="自定义探测端点"]').element.value).toBe('');
    expect(overlayButton(drawer.element, '保存').attributes('disabled')).toBeDefined();
    await drawer.get('[aria-label="自定义探测端点"]').setValue('/v1/images/generations');
    await overlayButton(drawer.element, '添加参数').trigger('click');
    expect(drawer.text()).toContain('请输入参数字段');
    expect(overlayButton(drawer.element, '保存').attributes('disabled')).toBeDefined();
    await drawer.get(parameterKeySelector('')).setValue('prompt');
    await drawer.get(parameterValueSelector('prompt')).setValue('ping');
    await overlayButton(drawer.element, '添加参数').trigger('click');
    await drawer.get(parameterKeySelector('')).setValue('n');
    await drawer.get(parameterValueSelector('n')).setValue('2');
    wrapper
      .getComponent(ModelProbeSettingsDrawer)
      .findAllComponents(Select)[2]
      .vm.$emit('update:value', 'number');
    await vi.waitFor(() => {
      expect(drawer.get('pre').text()).toContain(
        'POST https://api.openai.com/v1/images/generations',
      );
      expect(JSON.parse(drawer.get('pre').text().split('\n\n')[1])).toEqual({
        model: model.name,
        prompt: 'ping',
        n: 2,
      });
    });
    const request = {
      version: 2,
      endpoint: '/v1/images/generations',
      parameters: [
        { enabled: true, key: 'model', type: 'string', value: '', binding: 'model' },
        { enabled: true, key: 'prompt', type: 'string', value: 'ping' },
        { enabled: true, key: 'n', type: 'number', value: '2' },
      ],
    };
    await overlayButton(drawer.element, '保存').trigger('click');
    await flushPromises();
    expect(save).toHaveBeenCalledExactlyOnceWith(site.id, model.id, null, request);
    expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
    expect(details.selectedSiteDetail?.models[0].customRequest).toEqual(request);
    expect(details.selectedSiteDetail?.models[0].policy).toBeNull();
    expect(details.selectedSiteDetail?.policy).toEqual(site.policy);
    expect(details.selectedSiteDetail?.models[1]).toEqual(site.models[1]);
    expect(wrapper.getComponent(SiteDetail).text()).toContain('自定义请求');
    const modelRow = wrapper.get(`[data-row-key="${model.id}"]`);
    expect(modelRow.text()).not.toContain(`${model.firstTokenMs} ms`);
    expect(modelRow.text()).toContain('—');

    await button(wrapper, `设置模型 ${model.name}`).trigger('click');
    await flushPromises();
    const reopened = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(reopened.get('[aria-label="使用自定义探针请求"]').attributes('aria-checked')).toBe(
      'true',
    );
    expect(reopened.get<HTMLInputElement>('[aria-label="自定义探测端点"]').element.value).toBe(
      request.endpoint,
    );
    expect(reopened.get<HTMLInputElement>(parameterValueSelector('prompt')).element.value).toBe(
      'ping',
    );
    expect(reopened.get<HTMLInputElement>(parameterValueSelector('n')).element.value).toBe('2');
    await reopened.get(parameterValueSelector('prompt')).setValue('未保存草稿');
    await overlayButton(reopened.element, '取消').trigger('click');
    await flushPromises();
    const current = await api.fetchSite(site.id);
    current.models[0].customRequest!.parameters.find(
      (parameter) => parameter.key === 'prompt',
    )!.value = '当前模型的新配置';
    details.setSiteDetail(current);
    await button(wrapper, `设置模型 ${model.name}`).trigger('click');
    await flushPromises();
    expect(document.querySelector<HTMLInputElement>(parameterValueSelector('prompt'))!.value).toBe(
      '当前模型的新配置',
    );
  });

  it('关闭自定义后恢复常规探针配置，隐藏的端点错误不影响保存', async () => {
    const site = state.sites[0];
    site.models[0].customRequest = {
      version: 2,
      endpoint: '/custom',
      parameters: [{ enabled: true, key: 'input', type: 'string', value: 'ping' }],
    };
    const { wrapper, details } = await mountPage();
    const label = `设置模型 ${site.models[0].name}`;
    await button(wrapper, label).trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('[aria-label="自定义探测端点"]').setValue('');
    expect(overlayButton(drawer.element, '保存').attributes('disabled')).toBeDefined();
    await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
    expect(drawer.find('[aria-label="自定义探测端点"]').exists()).toBe(false);
    expect(drawer.get<HTMLInputElement>(parameterValueSelector('messages')).element.value).toBe(
      messagesValue(policyPrompt(site.policy)!),
    );
    expect(overlayButton(drawer.element, '保存').attributes('disabled')).toBeUndefined();
    await overlayButton(drawer.element, '保存').trigger('click');
    await flushPromises();
    expect(details.selectedSiteDetail?.models[0].policy).toEqual(site.policy);
    expect(details.selectedSiteDetail?.models[0].customRequest).toBeNull();
    const saved = await api.fetchSite(site.id);
    expect(saved.models[0].policy).toEqual(site.policy);
    expect(saved.models[0].customRequest).toBeNull();
    await button(wrapper, label).trigger('click');
    await flushPromises();
    expect(
      document.querySelector('[aria-label="使用自定义探针请求"]')!.getAttribute('aria-checked'),
    ).toBe('false');
  });

  it('关闭未保存草稿后重新打开，常规参数和自定义开关都使用当前站点模型配置', async () => {
    const { wrapper, details } = await mountPage();
    const label = `设置模型 ${details.selectedSiteDetail!.models[0].name}`;
    await button(wrapper, label).trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get(parameterValueSelector('messages')).setValue(messagesValue('未保存提示词'));
    await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
    await drawer.get('[aria-label="自定义探测端点"]').setValue('/unsaved');
    await overlayButton(drawer.element, '取消').trigger('click');
    await flushPromises();
    const current = await api.fetchSite('site-1');
    expect(current.models[0].policy).toBeNull();
    expect(details.selectedSiteDetail?.models[0].policy).toBeNull();
    setPolicyPrompt(current.policy, '最新站点默认策略');
    details.setSiteDetail(current);
    await button(wrapper, label).trigger('click');
    await flushPromises();
    const reopened = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(reopened.get('[aria-label="使用自定义探针请求"]').attributes('aria-checked')).toBe(
      'false',
    );
    expect(reopened.get<HTMLInputElement>(parameterValueSelector('messages')).element.value).toBe(
      messagesValue('最新站点默认策略'),
    );
    await reopened.get('[aria-label="使用自定义探针请求"]').trigger('click');
    expect(reopened.get<HTMLInputElement>('[aria-label="自定义探测端点"]').element.value).toBe('');
  });

  it('同步检查传当前模型名称；取消不写入，确认后写入并重新获取详情', async () => {
    const { wrapper, details } = await mountPage();
    const names = details.selectedSiteDetail!.models.map((model) => model.name);
    const check = vi.spyOn(api, 'syncModels');
    const apply = vi.spyOn(api, 'applyModelSync');
    const fetch = vi.spyOn(api, 'fetchSite');
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    expect(check).toHaveBeenCalledExactlyOnceWith('site-1', names);
    expect(wrapper.getComponent(ModelSyncModal).props('confirmation')?.removed[0].name).toBe(
      'legacy-demo',
    );
    expect(apply).not.toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
    await overlayButton(document.querySelector('.ant-modal')!, '取消同步').trigger('click');
    expect(wrapper.getComponent(ModelSyncModal).props('confirmation')).toBeUndefined();
    expect(details.selectedSiteDetail?.models).toHaveLength(3);
    expect(state.sites[0].models).toHaveLength(3);
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    await overlayButton(document.querySelector('.ant-modal')!, '确认同步').trigger('click');
    await flushPromises();
    expect(apply).toHaveBeenCalledExactlyOnceWith('site-1', {
      models: names,
      added: [],
      removed: ['model-0-2'],
    });
    expect(fetch).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(apply.mock.invocationCallOrder[0]).toBeLessThan(fetch.mock.invocationCallOrder[0]);
    expect(wrapper.getComponent(ModelSyncModal).props('confirmation')).toBeUndefined();
    expect(details.selectedSiteDetail?.models).toHaveLength(2);
    expect(wrapper.getComponent(SiteList).text()).toContain('2 个模型');
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
  });
  it.each([false, true])(
    '检查显示新增及删除模型（有删除：%s），仅新增也必须确认',
    async (hasRemoved) => {
      state.sites[0].models = state.sites[0].models.filter(
        (model) => model.name === 'gpt-demo' || (hasRemoved && model.name === 'legacy-demo'),
      );
      const before = JSON.stringify(state.sites[0]);
      const { wrapper } = await mountPage();
      const apply = vi.spyOn(api, 'applyModelSync');
      await button(wrapper, '同步模型').trigger('click');
      await flushPromises();
      const modal = new DOMWrapper(document.querySelector('.ant-modal')!);
      expect(modal.get('[aria-label="新增模型"]').text()).toContain('gpt-mini-demo');
      expect(modal.find('[aria-label="删除模型"]').exists()).toBe(hasRemoved);
      expect(apply).not.toHaveBeenCalled();
      expect(JSON.stringify(state.sites[0])).toBe(before);
      await overlayButton(modal.element, '确认同步').trigger('click');
      await flushPromises();
      expect(state.sites[0].models.map((model) => model.name)).toEqual([
        'gpt-demo',
        'gpt-mini-demo',
      ]);
    },
  );
  it('没有差异不显示确认框，也不调用写入接口', async () => {
    state.sites[0].models.pop();
    const { wrapper } = await mountPage();
    const apply = vi.spyOn(api, 'applyModelSync');
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    expect(wrapper.getComponent(ModelSyncModal).props('confirmation')).toBeUndefined();
    expect(apply).not.toHaveBeenCalled();
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
  });
  it('确认写入失败保留差异，重试成功后才刷新详情', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    window.history.replaceState({}, '', '/?scenario=action-error');
    const fetch = vi.spyOn(api, 'fetchSite');
    await overlayButton(document.querySelector('.ant-modal')!, '确认同步').trigger('click');
    await flushPromises();
    expect(fetch).not.toHaveBeenCalled();
    expect(wrapper.getComponent(ModelSyncModal).props('confirmation')).toBeDefined();
    expect(details.selectedSiteDetail?.models).toHaveLength(3);
    expect(state.sites[0].models).toHaveLength(3);
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
    window.history.replaceState({}, '', '/');
    await overlayButton(document.querySelector('.ant-modal')!, '确认同步').trigger('click');
    await flushPromises();
    expect(fetch).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(details.selectedSiteDetail?.models).toHaveLength(2);
  });
  it('写入已完成但详情尚未返回时继续保持任务互斥，界面只使用重新读取的数据', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    let resolve!: (site: Awaited<ReturnType<typeof api.fetchSite>>) => void;
    const read = vi.spyOn(api, 'fetchSite').mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const apply = vi.spyOn(api, 'applyModelSync');
    const probe = vi.spyOn(api, 'runProbe');
    const confirmation = wrapper.getComponent(ModelSyncModal);
    confirmation.vm.$emit('confirm');
    await flushPromises();
    expect(state.sites[0].models).toHaveLength(2);
    expect(details.currentDetail?.models).toHaveLength(3);
    expect(details.siteDetailsViewState).toBe('loading');
    expect(useSiteTasksStore().tasks['site-1']?.type).toBe('sync');
    // 绕过按钮直接触发旧确认，也不能再次发送写入。
    confirmation.vm.$emit('confirm');
    await flushPromises();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(probe).not.toHaveBeenCalled();
    const loaded = structuredClone(state.sites[0]);
    loaded.models[0].durationMs = 6789;
    resolve(loaded);
    await flushPromises();
    expect(read).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(details.selectedSiteDetail?.models).toHaveLength(2);
    expect(details.selectedSiteDetail?.models[0].durationMs).toBe(6789);
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
  });
  it('写入成功后详情读取失败，提供详情重试，不重复提交同步差异', async () => {
    const { wrapper, details } = await mountPage();
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    const apply = vi.spyOn(api, 'applyModelSync');
    const fetch = vi.spyOn(api, 'fetchSite').mockRejectedValueOnce(new Error('详情读取失败'));
    await overlayButton(document.querySelector('.ant-modal')!, '确认同步').trigger('click');
    await flushPromises();
    expect(state.sites[0].models).toHaveLength(2);
    expect(details.siteDetailsViewState).toBe('error');
    expect(wrapper.text()).toContain('详情读取失败');
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
    await details.loadSiteDetails();
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenCalledTimes(1);
    expect(details.selectedSiteDetail?.models).toHaveLength(2);
  });
  it.each(['探测已启用模型', '立即探测模型 gpt-demo'])(
    '%s 期间保持站点和模型操作互斥，完成后恢复',
    async (label) => {
      const { wrapper, details } = await mountPage();
      const result = await api.fetchSite('site-1');
      result.models[0].durationMs = 5678;
      let resolve!: (value: typeof result) => void;
      const probe = vi.spyOn(api, 'runProbe').mockReturnValueOnce(
        new Promise((done) => {
          resolve = done;
        }),
      );
      await button(wrapper, label).trigger('click');
      expect(probe).toHaveBeenCalledExactlyOnceWith(
        'site-1',
        label === '探测已启用模型' ? undefined : 'model-0-0',
      );
      expect(button(wrapper, '同步模型').attributes('disabled')).toBeDefined();
      expect(button(wrapper, '添加模型').attributes('disabled')).toBeDefined();
      expect(button(wrapper, '设置模型 gpt-demo').attributes('disabled')).toBeDefined();
      expect(wrapper.get('[aria-label="停用模型 gpt-demo"]').attributes('disabled')).toBeDefined();
      resolve(result);
      await flushPromises();
      expect(details.selectedSiteDetail?.models[0].durationMs).toBe(5678);
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(button(wrapper, '同步模型').attributes('disabled')).toBeUndefined();
      await wrapper.get('[aria-label="停用模型 gpt-demo"]').trigger('click');
      await flushPromises();
      expect(details.selectedSiteDetail?.models[0].enabled).toBe(false);
    },
  );
});
