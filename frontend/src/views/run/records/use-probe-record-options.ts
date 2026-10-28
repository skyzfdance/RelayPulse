import type { ProbeRecordOptions } from '@/types/domain/records';
import type { Ref } from 'vue';

import { computed, onScopeDispose, ref, shallowRef } from 'vue';

import { fetchProbeRecordOptions } from '@/api/run/records';

/** 只读取筛选目录，并按后端给出的站点与模型关系映射下拉选项。 */
export function useProbeRecordOptions(siteId: Ref<string>) {
  const data = shallowRef<ProbeRecordOptions>();
  const loading = ref(false);
  const error = ref('');
  let version = 0;
  let disposed = false;
  const siteOptions = computed(() => [
    { value: '', label: '全部站点' },
    ...(data.value?.sites ?? []).map((site) => ({ value: site.id, label: site.name })),
  ]);
  const modelOptions = computed(() => [
    { value: '', label: '全部模型' },
    ...(data.value?.models ?? [])
      .filter((model) => !siteId.value || model.siteIds.includes(siteId.value))
      .map((model) => ({ value: model.name, label: model.name })),
  ]);

  async function load() {
    if (disposed) return;
    const requestVersion = ++version;
    loading.value = true;
    error.value = '';
    try {
      const next = await fetchProbeRecordOptions();
      if (!disposed && requestVersion === version) data.value = next;
    } catch (cause) {
      if (!disposed && requestVersion === version)
        error.value = cause instanceof Error ? cause.message : '筛选选项加载失败';
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }
  onScopeDispose(() => {
    disposed = true;
    version += 1;
  });
  return { data, loading, error, siteOptions, modelOptions, load };
}
