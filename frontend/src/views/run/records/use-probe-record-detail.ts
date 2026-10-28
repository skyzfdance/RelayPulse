import type { ProbeRecordDetail } from '@/types/domain/records';
import type { MaybeRefOrGetter } from 'vue';

import { onScopeDispose, ref, shallowRef, toValue, watch } from 'vue';

import { fetchProbeRecordDetail } from '@/api/run/records';
import { RelayPulseBackendError } from '@/utils/backend-error';

/** 每次打开重新读取历史快照；关闭、换记录或离开页面后忽略旧响应。 */
export function useProbeRecordDetail(recordId: MaybeRefOrGetter<string | null>) {
  const detail = shallowRef<ProbeRecordDetail>();
  const loading = ref(false);
  const error = ref('');
  const notFound = ref(false);
  const retryable = ref(false);
  let version = 0;
  let disposed = false;

  async function reload() {
    if (disposed) return;
    const id = toValue(recordId);
    const requestVersion = ++version;
    detail.value = undefined;
    error.value = '';
    notFound.value = retryable.value = false;
    loading.value = id !== null;
    if (id === null) return;
    try {
      const next = await fetchProbeRecordDetail(id);
      if (disposed || requestVersion !== version) return;
      detail.value = next;
    } catch (cause) {
      if (disposed || requestVersion !== version) return;
      if (cause instanceof RelayPulseBackendError && cause.code === 'probe_record_not_found') {
        notFound.value = true;
      } else {
        error.value = cause instanceof Error ? cause.message : '暂时无法读取探测详情，请重试';
        retryable.value = !(cause instanceof RelayPulseBackendError) || cause.retryable;
      }
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }

  watch(() => toValue(recordId), reload, { immediate: true });
  onScopeDispose(() => {
    disposed = true;
    version += 1;
  });
  return { detail, loading, error, notFound, retryable, reload };
}
