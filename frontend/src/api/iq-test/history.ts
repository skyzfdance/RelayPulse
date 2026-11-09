import type { IqHistoryFilterOption, IqRun } from '@/types/domain/iq-test';

import { callBackend } from '../call-backend';

/** 筛选目录读取失败由历史页展示并提供重试。 */
export function fetchIqHistoryFilterOptions(): Promise<IqHistoryFilterOption[]> {
  return callBackend<IqHistoryFilterOption[]>('get_iq_history_filter_options', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}

/** 详情读取失败由详情区域展示。 */
export function fetchIqRun(id: string): Promise<IqRun> {
  return callBackend<IqRun>(
    'get_iq_run_details',
    { id },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}

export function deleteIqRuns(ids: string[]): Promise<void> {
  return callBackend<void>(
    'delete_iq_runs',
    { ids },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

export function openIqRawFile(entryId: string): Promise<void> {
  return callBackend<void>(
    'open_iq_raw_stream',
    { entryId },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}
