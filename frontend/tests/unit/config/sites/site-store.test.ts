import type { ManagedSite, SiteListItem } from '@/types/domain/sites';

import { flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as api from '@/api/config/sites';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import { deferred } from '../../../support/deferred';

function items(name: string): SiteListItem[] {
  return [
    {
      id: 'site-1',
      name,
      priority: 1,
      enabled: true,
      modelCount: 0,
      health: 'muted',
      statusText: '暂无模型',
    },
  ];
}

beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('删除当前站点', () => {
  it('删除期间切换到另一站点，完成后保留当前详情与试探任务', async () => {
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    const tasks = useSiteTasksStore();
    await store.loadSite();
    await flushPromises();
    const pending = deferred<void>();
    const deleteSite = api.deleteSite;
    vi.spyOn(api, 'deleteSite').mockImplementationOnce(async (siteId) => {
      await pending.promise;
      await deleteSite(siteId);
    });
    const deleting = store.removeSite();
    store.selectedSiteId = 'site-2';
    await details.loadSiteDetails();
    const current = details.currentDetail;
    tasks.register('site-2', { type: 'test-model-probe', modelId: 'model-1-0' });
    const fetchDetail = vi.spyOn(api, 'fetchSite');
    pending.resolve();
    await deleting;
    await flushPromises();
    expect(store.sites.map((site) => site.id)).toEqual(['site-2', 'site-3']);
    expect(store.selectedSiteId).toBe('site-2');
    expect(details.currentDetail).toBe(current);
    expect(details.siteDetailsViewState).toBe('ready');
    expect(fetchDetail).not.toHaveBeenCalled();
    expect(tasks.tasks['site-2']?.type).toBe('test-model-probe');
    expect(tasks.tasks['site-1']).toBeUndefined();
  });

  it('从选中 ID 删除站点，成功后刷新列表并加载新的选中详情', async () => {
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    await store.loadSite();
    store.selectedSiteId = 'site-2';
    await details.loadSiteDetails();
    const remove = vi.spyOn(api, 'deleteSite');
    const list = vi.spyOn(api, 'fetchSites');
    const detail = vi.spyOn(api, 'fetchSite');

    await store.removeSite();
    await flushPromises();

    expect(remove).toHaveBeenCalledExactlyOnceWith('site-2');
    expect(list).toHaveBeenCalledOnce();
    expect(store.sites.map((site) => site.id)).toEqual(['site-1', 'site-3']);
    expect(store.selectedSiteId).toBe('site-1');
    expect(detail).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(details.selectedSiteDetail?.id).toBe('site-1');
  });

  it('删除最后一个站点后清空选中项和详情，不请求空 ID 的详情', async () => {
    state.sites = state.sites.slice(0, 1);
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    await store.loadSite();
    await flushPromises();
    expect(details.currentDetail?.id).toBe('site-1');
    const detail = vi.spyOn(api, 'fetchSite');

    await store.removeSite();
    await flushPromises();

    expect(store.sites).toEqual([]);
    expect(store.sitesViewState).toBe('empty');
    expect(store.selectedSiteId).toBeNull();
    expect(details.currentDetail).toBeUndefined();
    expect(details.selectedSiteDetail).toBeUndefined();
    expect(details.siteDetailsViewState).toBe('empty');
    expect(detail).not.toHaveBeenCalled();
  });

  it('删除失败不刷新，保留已有列表、选中项和详情', async () => {
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    await store.loadSite();
    await flushPromises();
    const previousSites = store.sites;
    const previousDetail = details.currentDetail;
    vi.spyOn(api, 'deleteSite').mockRejectedValueOnce(new Error('删除失败'));
    const list = vi.spyOn(api, 'fetchSites');
    const detail = vi.spyOn(api, 'fetchSite');

    await store.removeSite();

    expect(list).not.toHaveBeenCalled();
    expect(detail).not.toHaveBeenCalled();
    expect(store.sites).toBe(previousSites);
    expect(store.selectedSiteId).toBe('site-1');
    expect(details.currentDetail).toBe(previousDetail);
  });

  it('没有选中站点或当前站点正在探测时不删除', async () => {
    const store = useSiteStore();
    const remove = vi.spyOn(api, 'deleteSite');
    await store.removeSite();
    await store.loadSite();
    useSiteTasksStore().register('site-1', { type: 'probe' });

    await store.removeSite();

    expect(remove).not.toHaveBeenCalled();
    expect(store.sites).toHaveLength(3);
  });
});

describe('站点列表串行合并刷新', () => {
  it('同一轮刷新合并读取；后来的 false 不取消前面要求的详情加载', async () => {
    const fetch = vi.spyOn(api, 'fetchSites').mockResolvedValue(items('最新列表'));
    const detail = vi.spyOn(api, 'fetchSite');
    const store = useSiteStore();

    await Promise.all([
      store.loadSite(),
      store.loadSite({ autoLoadDetails: false }),
      store.loadSite({ autoLoadDetails: true }),
      store.loadSite({ autoLoadDetails: false }),
    ]);

    expect(fetch).toHaveBeenCalledOnce();
    expect(store.sites[0].name).toBe('最新列表');
    expect(store.sitesLoaded).toBe(true);
    expect(detail).toHaveBeenCalledOnce();
    expect(detail).toHaveBeenCalledWith('site-1');
  });

  it('a 执行时合并 b、c；下一次请求中又有 d，继续串行读取，所有调用等待最后结果', async () => {
    const first = deferred<SiteListItem[]>();
    const second = deferred<SiteListItem[]>();
    const third = deferred<SiteListItem[]>();
    const fetch = vi
      .spyOn(api, 'fetchSites')
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
      .mockReturnValueOnce(third.promise);
    const store = useSiteStore();
    store.sites = items('原列表');
    store.sitesLoaded = true;
    const finished: string[] = [];

    const a = store.loadSite({ autoLoadDetails: false }).then(() => finished.push('a'));
    await flushPromises();
    const b = store.loadSite({ autoLoadDetails: false }).then(() => finished.push('b'));
    const c = store.loadSite({ autoLoadDetails: false }).then(() => finished.push('c'));
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(1);

    first.resolve(items('a 的旧响应'));
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(store.sites[0].name).toBe('原列表');
    expect(finished).toEqual([]);

    const d = store.loadSite({ autoLoadDetails: false }).then(() => finished.push('d'));
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(2);
    second.resolve(items('c 的旧响应'));
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(store.sites[0].name).toBe('原列表');
    expect(finished).toEqual([]);

    third.resolve(items('d 的最新响应'));
    await Promise.all([a, b, c, d]);
    expect(store.sites[0].name).toBe('d 的最新响应');
    expect(finished.sort()).toEqual(['a', 'b', 'c', 'd']);
  });

  it('过期请求失败仍继续后续刷新，最新请求失败保留列表，之后可以重试', async () => {
    const first = deferred<SiteListItem[]>();
    const second = deferred<SiteListItem[]>();
    const fetch = vi
      .spyOn(api, 'fetchSites')
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
      .mockResolvedValueOnce(items('重试成功'));
    const store = useSiteStore();
    store.sites = items('原列表');
    store.sitesLoaded = true;

    const a = store.loadSite({ autoLoadDetails: false });
    await flushPromises();
    const b = store.loadSite({ autoLoadDetails: false });
    first.reject(new Error('过期错误'));
    await flushPromises();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(store.sitesLoadError).toBe('');
    second.reject(new Error('最新请求失败'));
    await Promise.all([a, b]);
    expect(store.sitesLoadError).toBe('最新请求失败');
    expect(store.sites[0].name).toBe('原列表');
    expect(store.sitesViewState).toBe('ready');

    await store.loadSite({ autoLoadDetails: false });
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(store.sitesLoadError).toBe('');
    expect(store.sites[0].name).toBe('重试成功');
  });

  it('默认加载立即清空列表和详情，列表返回后选首项并加载详情，保留任务登记', async () => {
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    const tasks = useSiteTasksStore();
    store.sites = await api.fetchSites();
    store.sitesLoaded = true;
    store.selectedSiteId = 'site-2';
    details.currentDetail = structuredClone(state.sites[1]);
    details.siteDetailsLoadError = '上次的详情错误';
    tasks.register('site-2', { type: 'probe' });
    const list = deferred<SiteListItem[]>();
    const detail = deferred<ManagedSite>();
    vi.spyOn(api, 'fetchSites').mockReturnValueOnce(list.promise);
    const fetchDetail = vi.spyOn(api, 'fetchSite').mockReturnValueOnce(detail.promise);

    const loading = store.loadSite();
    expect(store.sites).toEqual([]);
    expect(store.sitesLoaded).toBe(false);
    expect(store.selectedSiteId).toBeNull();
    expect(details.currentDetail).toBeUndefined();
    expect(details.siteDetailsLoadError).toBe('');
    expect(details.siteDetailsViewState).toBe('loading');
    expect(fetchDetail).not.toHaveBeenCalled();
    expect(tasks.tasks['site-2']).toEqual({ type: 'probe' });

    list.resolve(items('最新列表'));
    await loading;
    expect(store.sitesViewState).toBe('ready');
    expect(store.selectedSiteId).toBe('site-1');
    expect(fetchDetail).toHaveBeenCalledExactlyOnceWith('site-1');
    expect(details.siteDetailsViewState).toBe('loading');
    detail.resolve(structuredClone(state.sites[0]));
    await flushPromises();
    expect(details.siteDetailsViewState).toBe('ready');
  });

  it('false 保留列表、选中项和详情，列表更新后不发起详情请求', async () => {
    const store = useSiteStore();
    const details = useSiteDetailsStore();
    store.sites = await api.fetchSites();
    store.sitesLoaded = true;
    store.selectedSiteId = 'site-2';
    details.currentDetail = structuredClone(state.sites[1]);
    const previousSites = store.sites;
    const previousDetail = details.currentDetail;
    const pending = deferred<SiteListItem[]>();
    vi.spyOn(api, 'fetchSites').mockReturnValueOnce(pending.promise);
    const fetchDetail = vi.spyOn(api, 'fetchSite');

    const loading = store.loadSite({ autoLoadDetails: false });
    expect(store.sites).toBe(previousSites);
    expect(store.selectedSiteId).toBe('site-2');
    expect(details.currentDetail).toBe(previousDetail);
    expect(store.sitesViewState).toBe('ready');
    pending.resolve([...previousSites].reverse());
    await loading;
    expect(store.selectedSiteId).toBe('site-2');
    expect(details.currentDetail).toBe(previousDetail);
    expect(fetchDetail).not.toHaveBeenCalled();
  });

  it('列表请求中追加 false 后只为最新列表加载详情', async () => {
    const first = deferred<SiteListItem[]>();
    const second = deferred<SiteListItem[]>();
    vi.spyOn(api, 'fetchSites')
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const fetchDetail = vi.spyOn(api, 'fetchSite');
    const store = useSiteStore();
    const initial = store.loadSite();
    await flushPromises();
    const refresh = store.loadSite({ autoLoadDetails: false });
    first.resolve(items('旧列表'));
    await flushPromises();
    expect(fetchDetail).not.toHaveBeenCalled();
    second.resolve(items('新列表'));
    await Promise.all([initial, refresh]);
    expect(store.sites[0].name).toBe('新列表');
    expect(fetchDetail).toHaveBeenCalledExactlyOnceWith('site-1');
  });

  it.each(['成功', '失败'])(
    '重新初始化时作废旧详情；旧请求%s不影响新详情的加载状态',
    async (result) => {
      const oldDetail = deferred<ManagedSite>();
      const nextDetail = deferred<ManagedSite>();
      const fetchDetail = vi
        .spyOn(api, 'fetchSite')
        .mockReturnValueOnce(oldDetail.promise)
        .mockReturnValueOnce(nextDetail.promise);
      const store = useSiteStore();
      const details = useSiteDetailsStore();
      await store.loadSite();
      expect(fetchDetail).toHaveBeenCalledTimes(1);
      // 旧详情还没有返回，新一轮列表和详情也可以正常开始。
      await store.loadSite();
      expect(fetchDetail).toHaveBeenCalledTimes(2);
      if (result === '成功') oldDetail.resolve(structuredClone(state.sites[0]));
      else oldDetail.reject(new Error('旧详情错误'));
      await flushPromises();
      expect(details.currentDetail).toBeUndefined();
      expect(details.siteDetailsViewState).toBe('loading');
      expect(details.siteDetailsLoadError).toBe('');
      nextDetail.resolve({ ...structuredClone(state.sites[0]), name: '最新详情' });
      await flushPromises();
      expect(details.currentDetail?.name).toBe('最新详情');
      expect(details.siteDetailsViewState).toBe('ready');
    },
  );

  it('自动加载遇到空列表或失败不请求详情，也不遗留自动加载要求', async () => {
    const fetch = vi
      .spyOn(api, 'fetchSites')
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(new Error('列表失败'))
      .mockResolvedValueOnce(items('静默重试'));
    const detail = vi.spyOn(api, 'fetchSite');
    const store = useSiteStore();
    await store.loadSite();
    expect(store.sitesViewState).toBe('empty');
    expect(store.selectedSiteId).toBeNull();
    await store.loadSite();
    expect(store.sitesViewState).toBe('error');
    await store.loadSite({ autoLoadDetails: false });
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(detail).not.toHaveBeenCalled();
  });

  it('接口同步抛错也会结束当前处理，下一次调用能重新加载', async () => {
    const fetch = vi
      .spyOn(api, 'fetchSites')
      .mockImplementationOnce(() => {
        throw new Error('同步失败');
      })
      .mockResolvedValueOnce([]);
    const store = useSiteStore();

    await store.loadSite();
    expect(store.sitesViewState).toBe('error');
    await store.loadSite();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(store.sitesViewState).toBe('empty');
    expect(store.sitesLoadError).toBe('');
  });
});
