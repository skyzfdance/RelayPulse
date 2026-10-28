/**
 * 后端错误契约归一化。
 *
 * Tauri 后端失败时返回结构化错误（错误码/分类/字段/可否重试/操作 ID），
 * backendError(cause) 把 invoke 抛出的异常归一为 RelayPulseBackendError，
 * 保住这些字段供 UI 判断重试与定位；非后端格式的意外错误会上报诊断并返回兜底值。
 * 消费方：api/call-backend.ts 与 api/events.ts。
 */
import { type FrontendErrorSource, reportFrontendError } from './diagnostics';

interface BackendErrorPayload {
  code: string;
  category: string;
  message: string;
  field?: string | null;
  retryable: boolean;
  operationId?: string | null;
}

/** 保留后端错误契约（错误码、分类、字段与操作 ID）的后端调用异常。 */
export class RelayPulseBackendError extends Error {
  readonly code: string;
  readonly category: string;
  readonly field: string | null;
  readonly retryable: boolean;
  readonly operationId: string | null;

  constructor(payload: BackendErrorPayload) {
    super(payload.message);
    this.name = 'RelayPulseBackendError';
    this.code = payload.code;
    this.category = payload.category;
    this.field = payload.field ?? null;
    this.retryable = payload.retryable;
    this.operationId = payload.operationId ?? null;
  }
}

export function backendError(
  cause: unknown,
  source: FrontendErrorSource = 'backend-invocation',
): Error {
  if (cause instanceof RelayPulseBackendError) return cause;
  if (isBackendErrorPayload(cause)) return new RelayPulseBackendError(cause);
  reportFrontendError(source, cause);
  return new RelayPulseBackendError({
    code: 'internal',
    category: 'internal',
    message: 'RelayPulse could not complete the operation.',
    retryable: false,
  });
}

function isBackendErrorPayload(value: unknown): value is BackendErrorPayload {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.code === 'string' &&
    typeof candidate.category === 'string' &&
    typeof candidate.message === 'string' &&
    typeof candidate.retryable === 'boolean'
  );
}
