import type {
  ProbeRecordListItem,
  ProbeRecordsQuery,
  ProbeResult,
  RecordsPage,
  RecordTimeRange,
} from '@/types/domain/records';
import type { Ref } from 'vue';

import { computed, onScopeDispose, reactive, ref, shallowRef } from 'vue';

import { clearProbeRecords, deleteProbeRecord, fetchProbeRecords } from '@/api/run/records';

/** 探测记录 Tab 的查询与操作状态；列表内容直接使用后端 DTO。 */
export function useProbeRecords(filters: { range: Ref<RecordTimeRange>; siteId: Ref<string> }) {
  const data = shallowRef<RecordsPage<ProbeRecordListItem>>();
  const loading = ref(false);
  const error = ref('');
  const modelName = ref('');
  const result = ref<ProbeResult | ''>('');
  const paging = reactive({ page: 1, pageSize: 10 });
  const mutation = ref<'delete' | 'clear' | null>(null);
  const busy = computed(() => mutation.value !== null);
  let version = 0;
  let disposed = false;

  async function load() {
    if (disposed) return;
    const requestVersion = ++version;
    const query: ProbeRecordsQuery = {
      range: filters.range.value,
      siteId: filters.siteId.value || null,
      modelName: modelName.value || null,
      result: result.value || null,
      ...paging,
    };
    loading.value = true;
    error.value = '';
    try {
      const next = await fetchProbeRecords(query);
      if (disposed || requestVersion !== version) return;
      data.value = next;
      paging.page = next.page;
      return next;
    } catch (cause) {
      if (!disposed && requestVersion === version)
        error.value = cause instanceof Error ? cause.message : '探测记录加载失败';
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }

  function invalidate(clearData = false) {
    version += 1;
    loading.value = false;
    if (clearData) data.value = undefined;
  }
  function changeModel(value: string) {
    modelName.value = value;
    paging.page = 1;
    void load();
  }
  function changeResult(value: ProbeResult | '') {
    result.value = value;
    paging.page = 1;
    void load();
  }
  function paginate(page: number, pageSize: number) {
    paging.page = pageSize === paging.pageSize ? page : 1;
    paging.pageSize = pageSize;
    void load();
  }
  async function mutate(kind: 'delete' | 'clear', recordId?: string): Promise<boolean> {
    if (disposed || busy.value) return false;
    mutation.value = kind;
    try {
      if (kind === 'clear') await clearProbeRecords();
      else await deleteProbeRecord(recordId!);
      if (disposed) return false;
      invalidate(true);
      return true;
    } catch {
      // API 已反馈错误；保留原列表和确认弹窗，供用户重试。
      return false;
    } finally {
      mutation.value = null;
    }
  }

  onScopeDispose(() => {
    disposed = true;
    invalidate();
  });
  return {
    data,
    loading,
    error,
    modelName,
    result,
    paging,
    mutation,
    busy,
    load,
    invalidate,
    changeModel,
    changeResult,
    paginate,
    removeRecord: (recordId: string) => mutate('delete', recordId),
    clearRecords: () => mutate('clear'),
  };
}
