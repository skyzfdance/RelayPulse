import type { MaybeRefOrGetter } from 'vue';

import { ref, toValue, watch } from 'vue';

import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { fetchProbeRecordResponse, openProbeRecordLogDirectory } from '@/api/run/records';
import { useFeedback } from '@/composables/useFeedback';

import { useProbeRecordContent } from './use-probe-record-content';

/** 原始返回按需读取；打开目录由探测记录模块独立处理。 */
export function useProbeRecordResponse(
  recordId: MaybeRefOrGetter<string>,
  expanded: MaybeRefOrGetter<boolean>,
) {
  const resource = useProbeRecordContent(
    recordId,
    expanded,
    (id) => fetchProbeRecordResponse(id),
    '暂时无法读取原始返回，请重试',
  );
  const { content } = resource;
  const openingDirectory = ref(false);
  const feedback = useFeedback();
  watch(
    content,
    () => {
      openingDirectory.value = false;
    },
    { flush: 'sync' },
  );
  async function openLogDirectory() {
    const file = content.value?.logFile;
    if (openingDirectory.value || !file?.canOpenDirectory) return;
    const isCurrent = resource.current();
    openingDirectory.value = true;
    try {
      const result = await openProbeRecordLogDirectory(toValue(recordId));
      if (isCurrent()) feedback.success(result.message);
    } catch (cause) {
      if (isCurrent()) {
        feedback.error(
          cause instanceof Error ? cause.message : '暂时无法打开日志目录，请重试',
          'message',
        );
        reportOperationReadFailure('probe_record_detail', 'detail', cause);
      }
    } finally {
      if (isCurrent()) openingDirectory.value = false;
    }
  }

  return { ...resource, openingDirectory, openLogDirectory };
}
