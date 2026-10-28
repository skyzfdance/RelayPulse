import type { SiteListItem } from '@/types/domain/sites';

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { deleteSite, fetchSites } from '@/api/config/sites';

import { useSiteDetailsStore } from './site-details';
import { useSiteTasksStore } from './site-tasks';

/** 站点管理的共享业务数据、展示状态与列表读取。 */
export const useSiteStore = defineStore('site', () => {
  /** 左侧摘要列表，按接口返回的优先级顺序保存，不包含模型明细。 */
  const sites = ref<SiteListItem[]>([]);

  /** 区分尚未取得列表与已加载但没有站点；不是请求中的 loading。 */
  const sitesLoaded = ref(false);

  /** 列表请求失败的原因，避免首次加载失败后一直显示骨架或误显示空列表。 */
  const sitesLoadError = ref('');

  /** 左右区域共享的选中项，null 表示未选择；不另存一份选中站点对象。 */
  const selectedSiteId = ref<string | null>(null);

  /** 首次取得列表前显示骨架；成功取得空数组后才显示添加站点入口。 */
  const sitesViewState = computed(() => {
    if (!sitesLoaded.value) return sitesLoadError.value ? 'error' : 'loading';
    return sites.value.length ? 'ready' : 'empty';
  });

  // 每次调用登记一次刷新；这是递增序号，不是队列数组的下标。
  let requestedVersion = 0;
  // 所有调用共同等待这一轮刷新结束，同时最多执行一个列表请求。
  let runningLoad: Promise<void> | null = null;
  // 合并刷新时保留自动加载详情的要求，后来的静默刷新不能取消它。
  let pendingDetails = false;

  /** 默认清空并重新加载；false 仅静默刷新列表。列表完成后，详情按独立状态加载。 */
  async function loadSite({ autoLoadDetails = true } = {}) {
    requestedVersion += 1;
    if (autoLoadDetails) {
      pendingDetails = true;
      sites.value = [];
      sitesLoaded.value = false;
      sitesLoadError.value = '';
      selectedSiteId.value = null;
      useSiteDetailsStore().resetSiteDetails();
    }

    if (!runningLoad) {
      // 先登记共享 Promise，再开始请求；同一轮同步调用也能合并。
      runningLoad = Promise.resolve().then(async () => {
        try {
          let version = 0;
          // 中途积累的多次刷新合成下一次请求；成功或失败都继续检查。
          while (version < requestedVersion) {
            version = requestedVersion;
            sitesLoadError.value = '';
            try {
              const next = await fetchSites();
              // 等待期间又登记了刷新，本次结果作废，继续读取最新列表。
              if (version !== requestedVersion) continue;
              const previousId = selectedSiteId.value;
              sites.value = next;
              if (!next.some((site) => site.id === selectedSiteId.value)) {
                selectedSiteId.value = next[0]?.id ?? null;
              }
              sitesLoaded.value = true;
              const selectionRemoved = previousId !== null && previousId !== selectedSiteId.value;
              if (pendingDetails || selectionRemoved) {
                pendingDetails = false;
                // 详情独立加载，不阻塞后续的列表刷新。
                void useSiteDetailsStore().loadSiteDetails();
              }
            } catch (error) {
              if (version !== requestedVersion) continue;
              pendingDetails = false;
              sitesLoadError.value = error instanceof Error ? error.message : '站点列表加载失败';
              // API 已提示；已有列表保留，首次失败由页面提供重试入口。
            }
          }
        } finally {
          runningLoad = null;
        }
      });
    }

    await runningLoad;
  }

  async function removeSite() {
    const siteId = selectedSiteId.value;
    const tasks = useSiteTasksStore();
    if (!siteId || !tasks.register(siteId, { type: 'delete-site' })) return;
    try {
      await deleteSite(siteId);
      await loadSite({ autoLoadDetails: false });
    } catch {
      // API 已提示，删除失败时保留当前列表和详情。
    } finally {
      tasks.remove(siteId);
    }
  }

  return {
    sites,
    sitesLoaded,
    sitesLoadError,
    selectedSiteId,
    sitesViewState,
    loadSite,
    removeSite,
  };
});
