import type { IqHistoryFilterOption } from '@/types/domain/iq-test';

import { onScopeDispose, ref, shallowRef } from 'vue';

import { fetchIqHistoryFilterOptions } from '@/api/iq-test/history';

/** 直接消费接口目录，不从历史列表或当前源配置拼装选项。 */
export function useIqHistoryFilterOptions() {
  const options = shallowRef<IqHistoryFilterOption[]>([]);
  const loading = ref(false);
  const error = ref('');
  let version = 0;
  let disposed = false;

  async function load() {
    if (disposed) return;
    const requestVersion = ++version;
    loading.value = true;
    error.value = '';
    try {
      const next = await fetchIqHistoryFilterOptions();
      if (!disposed && requestVersion === version) options.value = next;
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
  return { options, loading, error, load };
}
