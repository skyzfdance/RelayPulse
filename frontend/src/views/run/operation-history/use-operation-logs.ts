import type {
  OperationLogDetail,
  OperationLogPage,
  OperationLogQuery,
} from '@/types/domain/operation-logs';

import { onScopeDispose, ref, shallowRef } from 'vue';

import { fetchOperationLogDetail, fetchOperationLogs } from '@/api/run/operation-logs';

export function useOperationLogs() {
  const data = shallowRef<OperationLogPage>();
  const loading = ref(false);
  const error = ref('');
  const detail = shallowRef<OperationLogDetail>();
  const detailLoading = ref(false);
  const detailError = ref('');
  const detailMissing = ref(false);
  let listVersion = 0;
  let detailVersion = 0;
  let disposed = false;

  async function load(query: OperationLogQuery) {
    if (disposed) return;
    const version = ++listVersion;
    loading.value = true;
    error.value = '';
    try {
      const next = await fetchOperationLogs(query);
      if (disposed || version !== listVersion) return;
      data.value = next;
      return next;
    } catch (cause) {
      if (!disposed && version === listVersion)
        error.value = cause instanceof Error ? cause.message : '操作记录加载失败';
    } finally {
      if (!disposed && version === listVersion) loading.value = false;
    }
  }

  async function loadDetail(logId: string) {
    if (disposed) return;
    const version = ++detailVersion;
    detail.value = undefined;
    detailError.value = '';
    detailMissing.value = false;
    detailLoading.value = true;
    try {
      const next = await fetchOperationLogDetail(logId);
      if (disposed || version !== detailVersion) return;
      detail.value = next;
    } catch (cause) {
      if (disposed || version !== detailVersion) return;
      detailMissing.value = (cause as { category?: string })?.category === 'not_found';
      detailError.value = cause instanceof Error ? cause.message : '操作详情读取失败';
    } finally {
      if (!disposed && version === detailVersion) detailLoading.value = false;
    }
  }

  function closeDetail() {
    detailVersion += 1;
    detail.value = undefined;
    detailError.value = '';
    detailMissing.value = false;
    detailLoading.value = false;
  }

  onScopeDispose(() => {
    disposed = true;
    listVersion += 1;
    detailVersion += 1;
  });

  return {
    data,
    loading,
    error,
    detail,
    detailLoading,
    detailError,
    detailMissing,
    load,
    loadDetail,
    closeDetail,
  };
}
