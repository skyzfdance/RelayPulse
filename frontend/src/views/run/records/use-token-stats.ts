import type { RecordTimeRange, TokenStatsPage, TokenStatsQuery } from '@/types/domain/records';
import type { Ref } from 'vue';

import { onScopeDispose, reactive, ref, shallowRef } from 'vue';

import { fetchProbeTokenStats } from '@/api/run/records';

/** Token 统计 Tab 独立读取后端聚合结果，不从探测列表计算或补齐统计。 */
export function useTokenStats(filters: { range: Ref<RecordTimeRange>; siteId: Ref<string> }) {
  const data = shallowRef<TokenStatsPage>();
  const loading = ref(false);
  const error = ref('');
  const paging = reactive({ page: 1, pageSize: 10 });
  let version = 0;
  let disposed = false;

  async function load() {
    if (disposed) return;
    const requestVersion = ++version;
    const query: TokenStatsQuery = {
      range: filters.range.value,
      siteId: filters.siteId.value || null,
      ...paging,
    };
    loading.value = true;
    error.value = '';
    try {
      const next = await fetchProbeTokenStats(query);
      if (disposed || requestVersion !== version) return;
      data.value = next;
      paging.page = next.page;
      return next;
    } catch (cause) {
      if (!disposed && requestVersion === version)
        error.value = cause instanceof Error ? cause.message : 'Token 统计加载失败';
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }

  function invalidate(clearData = false) {
    version += 1;
    loading.value = false;
    if (clearData) data.value = undefined;
  }
  function paginate(page: number, pageSize: number) {
    paging.page = pageSize === paging.pageSize ? page : 1;
    paging.pageSize = pageSize;
    void load();
  }
  onScopeDispose(() => {
    disposed = true;
    invalidate();
  });
  return { data, loading, error, paging, load, invalidate, paginate };
}
