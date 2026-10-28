import type { CustomProbeTestResult, ManagedSite, SiteListItem } from '@/types/domain/sites';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';

import * as backend from '@/api/call-backend';
import * as api from '@/api/config/sites';
import * as globalLoading from '@/composables/useGlobalLoading';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { getMockHandler, registerMockHandler } from '@/mocks/handlers/registry';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import { deferred } from '../../../support/deferred';

import ModelProbeSettingsDrawer from '@/views/config/sites/components/ModelProbeSettingsDrawer.vue';
import SitesView from '@/views/config/sites/SitesView.vue';

beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
});
afterEach(() => {
  vi.restoreAllMocks();
});

async function mountPage() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/sites', component: SitesView },
      { path: '/records', component: { template: '<div>记录页面</div>' } },
    ],
  });
  await router.push('/sites');
  const wrapper = mount(defineComponent({ template: '<router-view />' }), {
    global: { plugins: [router, AntdvNext] },
  });
  await flushPromises();
  await vi.waitFor(() => expect(wrapper.findAll('[data-sort-id]')).toHaveLength(3));
  return { router, wrapper };
}
it('列表加载后展示所选站点详情；选择、小面板显示设置和键盘排序互不干扰', async () => {
  const commands = vi.spyOn(backend, 'callBackend');
  const { wrapper } = await mountPage();
  try {
    expect(wrapper.text()).toContain('3 个模型 · 全部正常');
    expect(wrapper.text()).toContain('基本信息');
    expect(wrapper.text()).toContain('同步模型');
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('OpenAI 主站');
    const row = () => wrapper.get('[data-sort-id="site-2"]');
    await row().trigger('click');
    expect(row().get('button[aria-current]').attributes('aria-current')).toBe('true');
    await wrapper.get('[data-sort-id="site-1"] [role="switch"]').trigger('click');
    await flushPromises();
    expect(row().get('button[aria-current]').attributes('aria-current')).toBe('true');
    const hiddenSite = wrapper.get('[data-sort-id="site-1"]');
    expect(hiddenSite.get('[role="switch"]').attributes('aria-checked')).toBe('false');
    expect(hiddenSite.text()).toContain('全部正常');
    await row().get('[data-sort-handle]').trigger('keydown', { key: 'ArrowUp' });
    await flushPromises();
    expect(
      wrapper.findAll('[data-sort-id]').map((item) => item.attributes('data-sort-id')),
    ).toEqual(['site-2', 'site-1', 'site-3']);
    expect(row().get('[data-sort-handle]').attributes('aria-label')).toContain('优先级 1');
    expect(row().get('button[aria-current]').attributes('aria-current')).toBe('true');
    expect(commands.mock.calls.some(([command]) => command === 'get_site')).toBe(true);
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('Anthropic');
  } finally {
    wrapper.unmount();
  }
});
it.each(['sync', 'probe', 'probe-model'] as const)(
  '%s 跨路由仍阻止修改本站点的小面板显示设置，结束后更新列表摘要',
  async (type) => {
    const store = useSiteTasksStore();
    store.register('site-1', type === 'probe-model' ? { type, modelId: 'model-0-0' } : { type });
    const { router, wrapper } = await mountPage();
    try {
      const toggle = () => wrapper.get('[data-sort-id="site-1"] [role="switch"]');
      expect(toggle().attributes('disabled')).toBeDefined();
      expect(
        wrapper.get('[data-sort-id="site-2"] [role="switch"]').attributes('disabled'),
      ).toBeUndefined();
      await router.push('/records');
      await flushPromises();
      expect(wrapper.text()).toBe('记录页面');
      expect(store.tasks['site-1']?.type).toBe(type);
      await router.push('/sites');
      await flushPromises();
      expect(toggle().attributes('disabled')).toBeDefined();
      state.sites[0].models = [];
      store.remove('site-1');
      await flushPromises();
      expect(toggle().attributes('disabled')).toBeUndefined();
      expect(wrapper.get('[data-sort-id="site-1"]').text()).toContain('0 个模型');
    } finally {
      wrapper.unmount();
    }
  },
);

it('详情未返回时右侧显示骨架，左侧仍可操作；详情成功后才显示内容', async () => {
  const pending = deferred<ManagedSite>();
  const fetch = vi.spyOn(api, 'fetchSite').mockReturnValueOnce(pending.promise);
  const global = vi.spyOn(globalLoading, 'acquireGlobalLoading');
  const { wrapper } = await mountPage();
  try {
    expect(fetch).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(3);
    expect(wrapper.find('[aria-label="正在加载站点详情"]').exists()).toBe(true);
    expect(wrapper.text()).not.toContain('基本信息');
    expect(wrapper.text()).not.toContain('还没有站点');
    expect(global).not.toHaveBeenCalled();
    pending.resolve(structuredClone(state.sites[0]));
    await flushPromises();
    expect(wrapper.find('[aria-label="正在加载站点详情"]').exists()).toBe(false);
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('OpenAI 主站');
    expect(useSiteDetailsStore().selectedSiteDetail?.id).toBe('site-1');
  } finally {
    wrapper.unmount();
  }
});

it('详情失败只在右侧显示错误；重试时恢复骨架，成功后展示详情', async () => {
  const retry = deferred<ManagedSite>();
  const fetch = vi
    .spyOn(api, 'fetchSite')
    .mockRejectedValueOnce(new Error('详情连接失败'))
    .mockReturnValueOnce(retry.promise);
  const { wrapper } = await mountPage();
  try {
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(3);
    expect(wrapper.get('[role="alert"]').text()).toContain('详情连接失败');
    expect(wrapper.text()).not.toContain('还没有站点');
    await wrapper.get('[role="alert"] button').trigger('click');
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(wrapper.find('[aria-label="正在加载站点详情"]').exists()).toBe(true);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    retry.resolve(structuredClone(state.sites[0]));
    await flushPromises();
    expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('OpenAI 主站');
  } finally {
    wrapper.unmount();
  }
});

it.each(['成功', '失败'])(
  '切换站点后，旧详情请求%s不结束当前骨架，也不覆盖当前详情',
  async (result) => {
    const first = deferred<ManagedSite>();
    const second = deferred<ManagedSite>();
    vi.spyOn(api, 'fetchSite')
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const { wrapper } = await mountPage();
    try {
      await wrapper.get('[data-sort-id="site-2"]').trigger('click');
      if (result === '成功') first.resolve(structuredClone(state.sites[0]));
      else first.reject(new Error('上个站点的错误'));
      await flushPromises();
      expect(wrapper.find('[aria-label="正在加载站点详情"]').exists()).toBe(true);
      expect(wrapper.find('input#name').exists()).toBe(false);
      expect(wrapper.find('[role="alert"]').exists()).toBe(false);
      second.resolve(structuredClone(state.sites[1]));
      await flushPromises();
      expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('Anthropic');
      expect(useSiteDetailsStore().selectedSiteDetail?.id).toBe('site-2');
    } finally {
      wrapper.unmount();
    }
  },
);

it('首次请求期间只显示站点骨架，不占全屏 loading，也不提供缓存详情', async () => {
  const items = await api.fetchSites();
  const original = getMockHandler('get_sites')!;
  let resolve!: (sites: SiteListItem[]) => void;
  const pending = new Promise<SiteListItem[]>((done) => {
    resolve = done;
  });
  registerMockHandler('get_sites', () => pending);
  const global = vi.spyOn(globalLoading, 'acquireGlobalLoading');
  const commands = vi.spyOn(backend, 'callBackend');
  const wrapper = mount(SitesView, { global: { plugins: [AntdvNext] } });
  const store = useSiteStore();
  const detailsStore = useSiteDetailsStore();
  try {
    detailsStore.currentDetail = structuredClone(state.sites[0]);
    store.selectedSiteId = 'site-1';
    await flushPromises();
    expect(wrapper.find('[aria-label="正在加载站点"]').exists()).toBe(true);
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(0);
    expect(wrapper.text()).not.toContain('还没有站点');
    expect(store.sitesLoaded).toBe(false);
    expect(detailsStore.selectedSiteDetail).toBeUndefined();
    expect(global).not.toHaveBeenCalled();
    expect(commands.mock.calls.some(([command]) => command === 'get_site')).toBe(false);
    resolve(items);
    await flushPromises();
    expect(store.sitesLoaded).toBe(true);
    expect(store.sitesViewState).toBe('ready');
    expect(detailsStore.selectedSiteDetail?.id).toBe('site-1');
    expect(wrapper.find('[aria-busy="true"]').exists()).toBe(false);
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(3);
    store.sites = store.sites.filter((site) => site.id !== 'site-1');
    expect(detailsStore.selectedSiteDetail).toBeUndefined();
  } finally {
    resolve(items);
    registerMockHandler('get_sites', original);
    wrapper.unmount();
  }
});

it('加载成功的空列表显示原版空态，添加第一个站点后进入列表', async () => {
  seedSites(true);
  const wrapper = mount(SitesView, { attachTo: document.body, global: { plugins: [AntdvNext] } });
  try {
    await vi.waitFor(() => expect(wrapper.text()).toContain('还没有站点'));
    expect(useSiteStore().sitesLoaded).toBe(true);
    expect(useSiteStore().sitesViewState).toBe('empty');
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(0);
    expect(wrapper.text()).toContain('连接成功后即可选择需要探测的模型');
    await wrapper.get('button').trigger('click');
    await vi.waitFor(() => expect(document.querySelector('.ant-modal input#name')).not.toBeNull());
    const modal = document.querySelector('.ant-modal')!;
    await new DOMWrapper(modal.querySelector('input#name')!).setValue('首个站点');
    await new DOMWrapper(modal.querySelector('input#baseUrl')!).setValue('https://example.com');
    await new DOMWrapper(modal.querySelector('input[type="password"]')!).setValue('test-secret');
    const submit = Array.from(modal.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('连接并获取模型'),
    )!;
    await new DOMWrapper(submit).trigger('click');
    await flushPromises();
    expect(useSiteStore().sitesViewState).toBe('ready');
    expect(wrapper.text()).not.toContain('还没有站点');
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(1);
    expect(wrapper.text()).toContain('首个站点');
  } finally {
    wrapper.unmount();
  }
});

it('首次加载失败停止骨架，不显示空态；重试期间恢复骨架并能成功加载', async () => {
  vi.spyOn(api, 'fetchSites').mockRejectedValueOnce(new Error('模拟连接失败'));
  const wrapper = mount(SitesView, { global: { plugins: [AntdvNext] } });
  try {
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('模拟连接失败');
    expect(wrapper.find('[aria-busy="true"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('还没有站点');
    expect(useSiteStore().sitesLoaded).toBe(false);
    let resolve!: (sites: SiteListItem[]) => void;
    const items = await api.fetchSites();
    vi.spyOn(api, 'fetchSites').mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    await wrapper.get('button').trigger('click');
    expect(wrapper.find('[aria-busy="true"]').exists()).toBe(true);
    resolve(items);
    await flushPromises();
    expect(useSiteStore().sitesViewState).toBe('ready');
    expect(wrapper.findAll('[data-sort-id]')).toHaveLength(3);
  } finally {
    wrapper.unmount();
  }
});

it.each(['成功', '失败'])(
  '试探期间阻止路由跳转、后退和站点切换，%s后才允许离开',
  async (outcome) => {
    const pending = deferred<CustomProbeTestResult>();
    vi.spyOn(api, 'testCustomProbe').mockReturnValueOnce(pending.promise);
    const { router, wrapper } = await mountPage();
    try {
      await router.push('/records');
      await router.push('/sites');
      await flushPromises();
      await vi.waitFor(() =>
        expect(wrapper.find('[aria-label="设置模型 gpt-demo"]').exists()).toBe(true),
      );
      await wrapper.get('[aria-label="设置模型 gpt-demo"]').trigger('click');
      await flushPromises();
      const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
      await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
      await drawer.get('[aria-label="自定义探测端点"]').setValue('/custom');
      await drawer
        .findAll('button')
        .find((item) => item.text().replace(/\s/gu, '') === '试探一次')!
        .trigger('click');
      await flushPromises();
      expect(useSiteTasksStore().tasks['site-1']?.type).toBe('test-model-probe');
      await router.push('/records');
      expect(router.currentRoute.value.path).toBe('/sites');
      router.back();
      await flushPromises();
      await flushPromises();
      expect(router.currentRoute.value.path).toBe('/sites');
      await wrapper.get('[data-sort-id="site-2"]').trigger('click');
      expect(useSiteStore().selectedSiteId).toBe('site-1');
      expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(true);
      if (outcome === '成功')
        pending.resolve({
          success: true,
          statusCode: 200,
          durationMs: 1250,
          message: '模拟成功',
          simulated: true,
        });
      else pending.reject(new Error('试探连接失败'));
      await flushPromises();
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      await router.push('/records');
      await flushPromises();
      expect(router.currentRoute.value.path).toBe('/records');
      expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
      await router.push('/sites');
      await flushPromises();
      expect(router.currentRoute.value.path).toBe('/sites');
    } finally {
      pending.resolve({
        success: true,
        statusCode: 200,
        durationMs: 1,
        message: '',
        simulated: true,
      });
      await flushPromises();
      wrapper.unmount();
    }
  },
);
