import type { ProbeRecordRequestContent } from '@/types/domain/records';
import type { MaybeRefOrGetter } from 'vue';

import { onScopeDispose, ref, shallowRef, toValue, watch } from 'vue';

import { fetchProbeRecordRequest } from '@/api/run/records';
import { RelayPulseBackendError } from '@/utils/backend-error';

/** 首次展开时读取，同一条详情中收起再展开复用结果；换记录或卸载后隔离旧响应。 */
export function useProbeRecordRequest(
  recordId: MaybeRefOrGetter<string>,
  expanded: MaybeRefOrGetter<boolean>,
) {
  const content = shallowRef<ProbeRecordRequestContent>();
  const loading = ref(false);
  const error = ref('');
  const retryable = ref(false);
  let version = 0;
  let disposed = false;

  async function reload() {
    if (disposed || loading.value) return;
    const requestVersion = ++version;
    const id = toValue(recordId);
    content.value = undefined;
    error.value = '';
    retryable.value = false;
    loading.value = true;
    try {
      const next = await fetchProbeRecordRequest(id);
      if (disposed || requestVersion !== version) return;
      content.value = next;
    } catch (cause) {
      if (disposed || requestVersion !== version) return;
      error.value = cause instanceof Error ? cause.message : '暂时无法读取请求详情，请重试';
      retryable.value = !(cause instanceof RelayPulseBackendError) || cause.retryable;
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }

  watch(
    () => toValue(recordId),
    () => {
      version += 1;
      content.value = undefined;
      error.value = '';
      loading.value = retryable.value = false;
    },
    { flush: 'sync' },
  );
  watch(
    () => [toValue(recordId), toValue(expanded)] as const,
    ([, open]) => {
      if (open && !content.value && !error.value && !loading.value) void reload();
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    disposed = true;
    version += 1;
  });
  return { content, loading, error, retryable, reload };
}
