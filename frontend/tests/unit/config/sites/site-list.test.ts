import type { SiteListItem } from '@/types/domain/sites';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as backend from '@/api/call-backend';
import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import * as globalLoading from '@/composables/useGlobalLoading';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteStore } from '@/stores/site';

import { deferred } from '../../../support/deferred';

import AddSiteModal from '@/views/config/sites/components/AddSiteModal.vue';
import SiteList from '@/views/config/sites/components/SiteList.vue';
import SitesView from '@/views/config/sites/SitesView.vue';

const dispose: (() => void)[] = [];
beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
  vi.restoreAllMocks();
});

async function mountList() {
  const store = useSiteStore();
  await store.loadSite({ autoLoadDetails: false });
  // 直接挂载列表，不需要页面 provide。
  const wrapper = mount(SiteList, { global: { plugins: [AntdvNext] } });
  dispose.push(() => wrapper.unmount());
  return { wrapper, store };
}

async function mountPage() {
  const wrapper = mount(SitesView, { global: { plugins: [AntdvNext] } });
  dispose.push(() => wrapper.unmount());
  await flushPromises();
  return wrapper;
}

async function operate(wrapper: Awaited<ReturnType<typeof mountList>>['wrapper'], action: string) {
  if (action === '排序') {
    await wrapper
      .get('[data-sort-id="site-2"] [data-sort-handle]')
      .trigger('keydown', { key: 'ArrowUp' });
  } else {
    await wrapper.get('[data-sort-id="site-1"] [role="switch"]').trigger('click');
  }
  await flushPromises();
}

describe('站点列表直接使用 store', () => {
  it('排序未完成时重复方向键不再发送请求，完成后可以再次排序', async () => {
    const { wrapper } = await mountList();
    const pending = deferred<SiteListItem[]>();
    const reorderSites = api.reorderSites;
    const reorder = vi.spyOn(api, 'reorderSites').mockImplementationOnce(async (ids) => {
      await pending.promise;
      return reorderSites(ids);
    });
    await operate(wrapper, '排序');
    const handle = wrapper.get('[data-sort-id="site-2"] [data-sort-handle]');
    expect(handle.attributes('disabled')).toBeDefined();
    // 人工派发 DOM 事件，验证事件处理器也有防护。
    handle.element.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
    await flushPromises();
    expect(reorder).toHaveBeenCalledOnce();
    pending.resolve([]);
    await flushPromises();
    expect(handle.attributes('disabled')).toBeUndefined();
    await wrapper
      .get('[data-sort-id="site-2"] [data-sort-handle]')
      .trigger('keydown', { key: 'ArrowDown' });
    await flushPromises();
    expect(reorder).toHaveBeenCalledTimes(2);
  });

  it('选择站点更新共享 ID 并请求详情，不改变列表；添加按钮只通知父组件', async () => {
    const commands = vi.spyOn(backend, 'callBackend');
    const { wrapper, store } = await mountList();
    const before = store.sites;
    await wrapper.get('[data-sort-id="site-2"]').trigger('click');
    expect(store.selectedSiteId).toBe('site-2');
    expect(store.sites).toBe(before);
    expect(commands.mock.calls.filter(([command]) => command === 'get_site')).toEqual([
      ['get_site', { siteId: 'site-2' }, expect.any(Object)],
    ]);
    await wrapper.get('[aria-label="添加站点"]').trigger('click');
    expect(wrapper.emitted('add')).toHaveLength(1);
  });

  it.each(['排序', '切换小面板显示'])(
    '%s 失败保留原列表，不调用 loadSite，也不显示全屏 loading',
    async (action) => {
      const { wrapper, store } = await mountList();
      const before = store.sites;
      const load = vi.spyOn(store, 'loadSite');
      const detail = vi.spyOn(api, 'fetchSite');
      const global = vi.spyOn(globalLoading, 'acquireGlobalLoading');
      window.history.replaceState({}, '', '/?scenario=action-error');
      await operate(wrapper, action);
      expect(store.sites).toBe(before);
      expect(load).not.toHaveBeenCalled();
      expect(detail).not.toHaveBeenCalled();
      expect(global).not.toHaveBeenCalled();
    },
  );

  it.each(['排序', '切换小面板显示'])(
    '%s 成功后静默读取列表；等待期间不使用写接口返回值修改数据',
    async (action) => {
      const { wrapper, store } = await mountList();
      store.selectedSiteId = 'site-2';
      const before = store.sites;
      const pending = deferred<SiteListItem[]>();
      const fetchSites = api.fetchSites;
      const fetch = vi.spyOn(api, 'fetchSites').mockReturnValueOnce(pending.promise);
      const detail = vi.spyOn(api, 'fetchSite');
      const global = vi.spyOn(globalLoading, 'acquireGlobalLoading');
      await operate(wrapper, action);
      expect(fetch).toHaveBeenCalledOnce();
      expect(store.sites).toBe(before);
      expect(store.sitesLoaded).toBe(true);
      expect(store.sitesViewState).toBe('ready');
      expect(global).not.toHaveBeenCalled();
      expect(wrapper.find('.ant-switch-loading').exists()).toBe(false);

      pending.resolve(await fetchSites());
      await flushPromises();
      expect(store.selectedSiteId).toBe('site-2');
      expect(detail).not.toHaveBeenCalled();
      if (action === '排序') {
        expect(store.sites.map((site) => site.id)).toEqual(['site-2', 'site-1', 'site-3']);
        expect(store.sites.map((site) => site.priority)).toEqual([1, 2, 3]);
      } else {
        expect(store.sites[0]).toMatchObject({ ...before[0], enabled: false });
      }
    },
  );

  it('切换小面板显示请求期间切换站点，操作完成后不刷新其他站点的详情', async () => {
    const { wrapper, store } = await mountList();
    const pending = deferred<SiteListItem>();
    const toggleSite = api.toggleSite;
    const toggle = vi.spyOn(api, 'toggleSite').mockReturnValueOnce(pending.promise);
    const detail = vi.spyOn(api, 'fetchSite');
    await operate(wrapper, '切换小面板显示');
    expect(toggle).toHaveBeenCalledWith('site-1', false);
    await wrapper.get('[data-sort-id="site-2"]').trigger('click');
    await flushPromises();
    expect(detail).toHaveBeenCalledExactlyOnceWith('site-2');
    detail.mockClear();

    pending.resolve(await toggleSite('site-1', false));
    await flushPromises();
    expect(store.selectedSiteId).toBe('site-2');
    expect(store.sites[0].enabled).toBe(false);
    expect(detail).not.toHaveBeenCalled();
  });

  it('较早的列表响应不能覆盖切换小面板显示完成后发起的刷新', async () => {
    const { wrapper, store } = await mountList();
    const stale = store.sites;
    const pending = deferred<SiteListItem[]>();
    const fetch = vi.spyOn(api, 'fetchSites').mockReturnValueOnce(pending.promise);
    const loading = store.loadSite({ autoLoadDetails: false });
    await flushPromises();
    await operate(wrapper, '切换小面板显示');
    expect(fetch).toHaveBeenCalledOnce();
    pending.resolve(stale);
    await loading;
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(store.sites[0].enabled).toBe(false);
  });

  it('接口摘要直接供列表展示，不由前端重新聚合', async () => {
    const summaries = await api.fetchSites();
    summaries[0] = { ...summaries[0], modelCount: 18, health: 'error', statusText: '7 个异常' };
    vi.spyOn(api, 'fetchSites').mockResolvedValueOnce(summaries);
    const { wrapper } = await mountList();
    expect(wrapper.get('[data-sort-id="site-1"]').text()).toContain('18 个模型 · 7 个异常');
  });

  it('旧页面的响应不能覆盖重新进入页面时请求的数据', async () => {
    const first = await mountPage();
    const store = useSiteStore();
    const stale = store.sites;
    const pending = deferred<SiteListItem[]>();
    vi.spyOn(api, 'fetchSites').mockReturnValueOnce(pending.promise);
    const loading = store.loadSite({ autoLoadDetails: false });
    await flushPromises();
    first.unmount();
    dispose.pop();
    state.sites[0].name = '新页面取得的名称';
    const next = await mountPage();
    expect(next.find('[aria-label="正在加载站点"]').exists()).toBe(true);
    pending.resolve(stale);
    await loading;
    await flushPromises();
    expect(next.text()).toContain('新页面取得的名称');
    expect(store.sitesViewState).toBe('ready');
  });

  it('重新进入页面时弹窗关闭，重新加载列表并选择首个站点', async () => {
    const first = await mountPage();
    useSiteStore().selectedSiteId = 'site-2';
    await first.get('[aria-label="添加站点"]').trigger('click');
    expect(first.findComponent(AddSiteModal).exists()).toBe(true);
    first.unmount();
    dispose.pop();
    state.sites[0].name = '最新名称';
    const next = await mountPage();
    expect(next.findComponent(AddSiteModal).exists()).toBe(false);
    expect(useSiteStore().selectedSiteId).toBe('site-1');
    expect(next.text()).toContain('最新名称');
  });
});
