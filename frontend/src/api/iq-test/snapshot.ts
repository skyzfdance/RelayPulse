import type { IqSnapshot } from '@/types/domain/iq-test';

import { callBackend } from '../call-backend';

/** 补取整个 IQ 模块的现有快照契约，失败由页面展示。 */
export function fetchIqSnapshot(): Promise<IqSnapshot> {
  return callBackend<IqSnapshot>('get_iq_test_snapshot', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}
