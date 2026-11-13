import type { MaybeRefOrGetter } from 'vue';

import { computed, toValue, watch } from 'vue';

import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { useLazyResource } from '@/composables/useLazyResource';
import { RelayPulseBackendError } from '@/utils/backend-error';

/** 探测详情的错误展示与可见失败审计，不进入公共加载机制。 */
export function useProbeRecordContent<T>(
  recordId: MaybeRefOrGetter<string>,
  expanded: MaybeRefOrGetter<boolean>,
  fetcher: (id: string) => Promise<T>,
  fallback: string,
) {
  const resource = useLazyResource(recordId, expanded, fetcher);
  const error = computed(() => {
    const failure = resource.failure.value;
    if (!failure) return '';
    return failure.cause instanceof Error ? failure.cause.message : fallback;
  });
  const retryable = computed(() => {
    const failure = resource.failure.value;
    return Boolean(
      failure && (!(failure.cause instanceof RelayPulseBackendError) || failure.cause.retryable),
    );
  });
  let reported: { cause: unknown } | undefined;
  watch(
    () => [resource.failure.value, toValue(expanded)] as const,
    ([failure, open]) => {
      if (!failure) reported = undefined;
      else if (open && failure !== reported) {
        reportOperationReadFailure('probe_record_detail', 'detail', failure.cause);
        reported = failure;
      }
    },
  );
  return {
    content: resource.content,
    loading: resource.loading,
    error,
    retryable,
    reload: resource.reload,
    current: resource.current,
  };
}
