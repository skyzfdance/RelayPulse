import type { IqCallResult, IqRun } from '@/types/domain/iq-test';

/** 浏览器 Mock 的后端汇总代替实现，页面只消费汇总后的字段。 */
export function summarizeIqCallResult(run: Pick<IqRun, 'status' | 'entries'>): IqCallResult {
  const counts = { success: 0, failed: 0, timeout: 0, aborted: 0, notStarted: 0 };
  for (const entry of run.entries) {
    if (entry.status === 'completed') counts.success += 1;
    else if (entry.status === 'error') counts.failed += 1;
    else if (entry.status === 'timeout') counts.timeout += 1;
    else if (entry.status === 'aborted') {
      if (entry.attempt > 0) counts.aborted += 1;
      else counts.notStarted += 1;
    }
  }
  let value: IqCallResult['value'] = 'all-success';
  if (run.status === 'aborted') value = 'aborted';
  else if (counts.failed + counts.timeout > 0) {
    value = counts.success > 0 ? 'partial-failure' : 'all-failure';
  }
  return { value, ...counts };
}
