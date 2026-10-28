import type { ManagedSite } from '@/types/domain/sites';

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { fetchSite } from '@/api/config/sites';

import { useSiteStore } from './site';

/** 站点详情及其读取状态；表单草稿留在编辑区域。 */
export const useSiteDetailsStore = defineStore('site-details', () => {
  const siteStore = useSiteStore();

  /**
   * 当前选中站点的详情。
   * 连接配置、默认探针策略和模型列表均在 ManagedSite 内，模型独立策略随模型保存。
   * 这里只存已保存的数据快照，表单草稿和凭据输入留在编辑区域。
   */
  const currentDetail = ref<ManagedSite | undefined>(undefined);
  const siteDetailsLoading = ref(false);
  const siteDetailsLoadError = ref('');
  let loadVersion = 0;

  /** 列表未就绪或选中项已不在列表中时，清空详情。 */
  const selectedSiteDetail = computed(() => {
    const id = siteStore.selectedSiteId;
    if (!siteStore.sitesLoaded || !id || !siteStore.sites.some((site) => site.id === id)) {
      return undefined;
    }
    return currentDetail.value?.id === id ? currentDetail.value : undefined;
  });

  const siteDetailsViewState = computed(() => {
    if (!siteStore.sitesLoaded) return siteStore.sitesLoadError ? 'error' : 'loading';
    const id = siteStore.selectedSiteId;
    if (!id || !siteStore.sites.some((site) => site.id === id)) return 'empty';
    if (siteDetailsLoading.value) return 'loading';
    if (siteDetailsLoadError.value) return 'error';
    return selectedSiteDetail.value ? 'ready' : 'loading';
  });

  function resetSiteDetails() {
    // 清空时同时作废未完成的详情请求，避免旧响应重新填回数据。
    ++loadVersion;
    currentDetail.value = undefined;
    siteDetailsLoading.value = false;
    siteDetailsLoadError.value = '';
  }

  async function loadSiteDetails({ silent = false } = {}) {
    const version = ++loadVersion;
    const id = siteStore.selectedSiteId;
    siteDetailsLoadError.value = '';
    if (!siteStore.sitesLoaded || !id || !siteStore.sites.some((site) => site.id === id)) {
      siteDetailsLoading.value = false;
      currentDetail.value = undefined;
      return;
    }
    siteDetailsLoading.value = !silent || !selectedSiteDetail.value;
    try {
      const site = await fetchSite(id);
      if (version !== loadVersion || id !== siteStore.selectedSiteId) return;
      currentDetail.value = site;
    } catch (error) {
      if (version !== loadVersion || id !== siteStore.selectedSiteId) return;
      siteDetailsLoadError.value = error instanceof Error ? error.message : '站点详情加载失败';
    } finally {
      if (version === loadVersion) siteDetailsLoading.value = false;
    }
  }

  function setSiteDetail(site: ManagedSite) {
    if (site.id === siteStore.selectedSiteId) {
      currentDetail.value = site;
      // 写操作已经返回新详情，之前尚未返回的读取结果不能覆盖它。
      ++loadVersion;
      siteDetailsLoading.value = false;
      siteDetailsLoadError.value = '';
    }
  }

  return {
    currentDetail,
    selectedSiteDetail,
    siteDetailsLoading,
    siteDetailsLoadError,
    siteDetailsViewState,
    resetSiteDetails,
    loadSiteDetails,
    setSiteDetail,
  };
});
