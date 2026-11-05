import type { ReadFailureTarget } from '@/types/domain/operation-logs';

import { invoke, isTauri } from '@tauri-apps/api/core';

import { isBackendErrorPayload, RelayPulseBackendError } from '@/utils/backend-error';

/** 由所属功能确认错误已展示后调用；只发送固定目标，不传异常、URL 或业务参数。 */
export function reportOperationReadFailure(
  target: ReadFailureTarget,
  kind: 'page' | 'detail',
  cause: unknown,
): void {
  if (!isTauri()) return;
  if (
    cause instanceof RelayPulseBackendError
      ? cause.origin === 'backend'
      : isBackendErrorPayload(cause)
  )
    return;
  // 直接调用受限命令；失败静默结束，不递归采集，也不覆盖原读取结果。
  void invoke('report_operation_read_failure', { input: { target, kind } }).catch(() => undefined);
}
