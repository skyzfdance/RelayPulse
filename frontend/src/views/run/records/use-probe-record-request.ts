import type { MaybeRefOrGetter } from 'vue';

import { fetchProbeRecordRequest } from '@/api/run/records';

import { useProbeRecordContent } from './use-probe-record-content';

/** 首次展开时读取，同一条详情中收起再展开复用结果。 */
export function useProbeRecordRequest(
  recordId: MaybeRefOrGetter<string>,
  expanded: MaybeRefOrGetter<boolean>,
) {
  return useProbeRecordContent(
    recordId,
    expanded,
    (id) => fetchProbeRecordRequest(id),
    '暂时无法读取请求详情，请重试',
  );
}
