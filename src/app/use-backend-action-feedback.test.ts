import { describe, expect, it } from 'vitest';
import { RelayPulseBackendError } from '../data/tauri-relay-pulse-data-source';
import { backendActionErrorDescription } from './use-backend-action-feedback';

describe('backendActionErrorDescription', () => {
  it('adds a stable backend code without exposing internal details', () => {
    const error = new RelayPulseBackendError({
      code: 'database_unavailable',
      category: 'database',
      message: 'The RelayPulse database is unavailable.',
      retryable: true,
    });

    expect(backendActionErrorDescription(error, '站点没有创建。')).toBe(
      '站点没有创建。 错误代码：database_unavailable。',
    );
  });

  it('keeps the operation-specific fallback for unknown errors', () => {
    expect(backendActionErrorDescription(new Error('unknown'), '请稍后重试。')).toBe(
      '请稍后重试。',
    );
  });
});
