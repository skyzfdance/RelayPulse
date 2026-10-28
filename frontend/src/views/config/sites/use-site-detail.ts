import type { SiteTask } from '@/stores/site/site-tasks';
import type { ManagedSite, ModelSyncChanges, SiteDraft } from '@/types/domain/sites';

import { storeToRefs } from 'pinia';
import { computed, onScopeDispose, ref, watch } from 'vue';

import * as api from '@/api/config/sites';
import { useFeedback } from '@/composables/useFeedback';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import { siteDraft, validateSite } from './site-policy';

/** 详情的编辑草稿、弹窗和操作；组件直接从 store 读取公共数据。 */
export function useSiteDetail() {
  const store = useSiteStore();
  const { selectedSiteId: selectedId } = storeToRefs(store);
  const detailsStore = useSiteDetailsStore();
  const { selectedSiteDetail: selected } = storeToRefs(detailsStore);
  const taskStore = useSiteTasksStore();
  const siteBusy = computed(() => Boolean(selectedId.value && taskStore.tasks[selectedId.value]));
  const savingBasic = computed(() =>
    Boolean(selectedId.value && taskStore.tasks[selectedId.value]?.type === 'save-basic'),
  );
  // 重新进入页面时已有的任务，结束后需重新读取详情；本页操作自行回填结果。
  const inheritedTasks = new Set(Object.keys(taskStore.tasks));
  let disposed = false;

  const draft = ref<SiteDraft>();
  const baseline = ref<string>();
  const dirty = computed(
    () => Boolean(selected.value && draft.value) && JSON.stringify(draft.value) !== baseline.value,
  );
  const formError = ref('');
  const advancedOpen = ref(false);
  const sitePolicyOpen = ref(false);
  const addModelOpen = ref(false);
  const modelSettingsId = ref('');
  const editingModel = computed(() =>
    selected.value?.models.find((model) => model.id === modelSettingsId.value),
  );
  const syncConfirmation = ref<SyncConfirmation>();

  function canOperate(siteId: string, requireSaved = false) {
    return (
      !disposed &&
      siteId === selectedId.value &&
      Boolean(selected.value) &&
      !taskStore.tasks[siteId] &&
      (!requireSaved || !dirty.value)
    );
  }
  function openAdvancedSettings() {
    if (selectedId.value && canOperate(selectedId.value)) advancedOpen.value = true;
  }
  function openSitePolicy() {
    if (selectedId.value && canOperate(selectedId.value)) sitePolicyOpen.value = true;
  }
  function openAddModel() {
    if (selectedId.value && canOperate(selectedId.value, true)) addModelOpen.value = true;
  }
  function openModelSettings(modelId: string) {
    if (selectedId.value && canOperate(selectedId.value, true)) modelSettingsId.value = modelId;
  }
  function closeModelSettings() {
    if (selectedId.value && taskStore.tasks[selectedId.value]?.type === 'test-model-probe') return;
    modelSettingsId.value = '';
  }
  function resetDraft() {
    draft.value = selected.value ? siteDraft(selected.value) : undefined;
    baseline.value = JSON.stringify(draft.value);
    formError.value = '';
  }
  function closeDetailPanels() {
    advancedOpen.value = false;
    sitePolicyOpen.value = false;
    addModelOpen.value = false;
    modelSettingsId.value = '';
    syncConfirmation.value = undefined;
  }
  function applySite(site: ManagedSite) {
    if (disposed) return;
    detailsStore.setSiteDetail(site);
    void store.loadSite({ autoLoadDetails: false });
  }
  watch(
    selectedId,
    () => {
      closeDetailPanels();
      resetDraft();
    },
    { immediate: true },
  );
  watch(selected, () => {
    if (!dirty.value) resetDraft();
  });
  watch(
    () => Object.keys(taskStore.tasks),
    (current, previous) => {
      for (const id of previous) {
        if (!current.includes(id) && inheritedTasks.delete(id) && id === selectedId.value) {
          void detailsStore.loadSiteDetails();
          void store.loadSite({ autoLoadDetails: false });
        }
      }
    },
    { flush: 'sync' },
  );

  async function updateSite(siteId: string, task: SiteTask, request: () => Promise<ManagedSite>) {
    if (!taskStore.register(siteId, task)) return false;
    try {
      applySite(await request());
      return true;
    } catch {
      // API 已提示；失败保留原配置和编辑草稿。
      return false;
    } finally {
      taskStore.remove(siteId);
    }
  }
  async function saveSelectedSite(): Promise<boolean> {
    const siteId = selectedId.value;
    if (!siteId || !draft.value || !dirty.value || !canOperate(siteId)) return false;
    formError.value = validateSite(draft.value) ?? '';
    if (formError.value) return false;
    const input = { ...draft.value };
    const saved = await updateSite(siteId, { type: 'save-basic' }, () =>
      api.saveSite(siteId, input),
    );
    if (saved && !disposed && selectedId.value === siteId) resetDraft();
    return saved;
  }
  async function createModel(siteId: string, name: string): Promise<boolean> {
    if (!canOperate(siteId, true) || !name.trim()) return false;
    return updateSite(siteId, { type: 'add-model' }, () => api.addModel(siteId, name));
  }
  async function removeModel(siteId: string, modelId: string) {
    if (!canOperate(siteId, true)) return;
    return updateSite(siteId, { type: 'delete-model' }, () => api.deleteModel(siteId, modelId));
  }
  async function setModelEnabled(siteId: string, modelId: string, enabled: boolean) {
    if (!canOperate(siteId, true)) return;
    return updateSite(siteId, { type: 'toggle-model' }, () =>
      api.toggleModel(siteId, modelId, enabled),
    );
  }
  async function synchronizeModels(siteId: string) {
    if (!canOperate(siteId, true) || !taskStore.register(siteId, { type: 'sync' })) return;
    const models = selected.value!.models.map((model) => model.name);
    syncConfirmation.value = undefined;
    try {
      const changes = await api.syncModels(siteId, models);
      if (disposed || selectedId.value !== siteId) return;
      if (changes.added.length || changes.removed.length) {
        syncConfirmation.value = {
          ...changes,
          models,
          siteId,
          siteName: selected.value?.name ?? '',
        };
      } else {
        useFeedback().info('模型目录没有变化');
      }
    } catch {
      // API 已提示；检查失败不会改动站点数据。
    } finally {
      taskStore.remove(siteId);
    }
  }
  async function confirmModelSync() {
    const confirmation = syncConfirmation.value;
    if (!confirmation || !canOperate(confirmation.siteId, true)) return;
    const { siteId, models, added, removed } = confirmation;
    if (!taskStore.register(siteId, { type: 'sync' })) return;
    try {
      await api.applyModelSync(siteId, {
        models,
        added,
        removed: removed.map((model) => model.id),
      });
      if (disposed) return;
      if (selectedId.value === siteId) {
        syncConfirmation.value = undefined;
        // 写入接口没有详情返回值；保持任务互斥，直到重新取得详情和列表摘要。
        await detailsStore.loadSiteDetails();
      }
      await store.loadSite({ autoLoadDetails: false });
      useFeedback().success('模型目录已同步');
    } catch {
      // API 已提示；写入失败保留差异供重试，也可以取消后重新检查。
    } finally {
      taskStore.remove(siteId);
    }
  }
  async function probeModels(siteId: string, modelId?: string) {
    if (!canOperate(siteId, true)) return;
    if (!modelId && !selected.value?.models.some((model) => model.enabled)) return;
    const task = modelId ? { type: 'probe-model' as const, modelId } : { type: 'probe' as const };
    return updateSite(siteId, task, () => api.runProbe(siteId, modelId));
  }

  function beforeUnload(event: BeforeUnloadEvent) {
    if (
      dirty.value ||
      (selectedId.value && taskStore.tasks[selectedId.value]?.type === 'test-model-probe')
    ) {
      event.preventDefault();
      event.returnValue = '';
    }
  }
  window.addEventListener('beforeunload', beforeUnload);
  onScopeDispose(() => {
    disposed = true;
    window.removeEventListener('beforeunload', beforeUnload);
  });

  return {
    draft,
    dirty,
    formError,
    saveSelectedSite,
    advancedOpen,
    sitePolicyOpen,
    addModelOpen,
    modelSettingsId,
    editingModel,
    syncConfirmation,
    siteBusy,
    savingBasic,
    openAdvancedSettings,
    openSitePolicy,
    openAddModel,
    openModelSettings,
    closeModelSettings,
    createModel,
    removeModel,
    setModelEnabled,
    synchronizeModels,
    confirmModelSync,
    probeModels,
  };
}

export type SyncConfirmation = ModelSyncChanges & {
  models: string[];
  siteId: string;
  siteName: string;
};
