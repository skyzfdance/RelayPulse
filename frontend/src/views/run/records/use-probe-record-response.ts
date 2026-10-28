import type { ProbeRecordResponseContent } from '@/types/domain/records';
import type { MaybeRefOrGetter } from 'vue';

import { onScopeDispose, ref, shallowRef, toValue, watch } from 'vue';

import { fetchProbeRecordResponse, openProbeRecordLogDirectory } from '@/api/run/records';
import { useFeedback } from '@/composables/useFeedback';
import { RelayPulseBackendError } from '@/utils/backend-error';

/** 原始返回按需读取；元数据和打开结果均隔离已关闭或已切换记录的旧响应。 */
export function useProbeRecordResponse(
  recordId: MaybeRefOrGetter<string>,
  expanded: MaybeRefOrGetter<boolean>,
) {
  const content = shallowRef<ProbeRecordResponseContent>();
  const loading = ref(false);
  const error = ref('');
  const retryable = ref(false);
  const openingDirectory = ref(false);
  const feedback = useFeedback();
  let version = 0;
  let disposed = false;

  async function reload() {
    if (disposed || loading.value) return;
    const requestVersion = ++version;
    const id = toValue(recordId);
    content.value = undefined;
    error.value = '';
    retryable.value = false;
    openingDirectory.value = false;
    loading.value = true;
    try {
      const next = await fetchProbeRecordResponse(id);
      if (disposed || requestVersion !== version) return;
      content.value = next;
    } catch (cause) {
      if (disposed || requestVersion !== version) return;
      error.value = cause instanceof Error ? cause.message : '暂时无法读取原始返回，请重试';
      retryable.value = !(cause instanceof RelayPulseBackendError) || cause.retryable;
    } finally {
      if (!disposed && requestVersion === version) loading.value = false;
    }
  }

  async function openLogDirectory() {
    const file = content.value?.logFile;
    if (disposed || openingDirectory.value || !file?.canOpenDirectory) return;
    const actionVersion = version;
    openingDirectory.value = true;
    try {
      const result = await openProbeRecordLogDirectory(toValue(recordId));
      if (!disposed && actionVersion === version) feedback.success(result.message);
    } catch (cause) {
      if (!disposed && actionVersion === version)
        feedback.error(
          cause instanceof Error ? cause.message : '暂时无法打开日志目录，请重试',
          'message',
        );
    } finally {
      if (!disposed && actionVersion === version) openingDirectory.value = false;
    }
  }

  watch(
    () => toValue(recordId),
    () => {
      version += 1;
      content.value = undefined;
      error.value = '';
      loading.value = retryable.value = false;
      openingDirectory.value = false;
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
  return { content, loading, error, retryable, reload, openingDirectory, openLogDirectory };
}
