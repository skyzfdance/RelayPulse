import { invoke } from '@tauri-apps/api/core';
import type { InvokeArgs } from '@tauri-apps/api/core';
import { RelayPulseBackendError, backendError } from '../../data/tauri-relay-pulse-data-source';
import type {
  OperationHistoryDataSource,
  OperationHistoryErrorCode,
  OperationHistoryPageResult,
  OperationHistoryQuery,
  OperationRecordDetails,
} from './operation-history-types';
import { OperationHistoryError } from './operation-history-types';

type InvokeOperationHistoryCommand = <T>(command: string, args?: InvokeArgs) => Promise<T>;

const messages: Record<OperationHistoryErrorCode, string> = {
  cursor_expired: '操作记录已更新，正在重新加载',
  query_busy: '操作记录查询繁忙，请稍后重试',
  invalid_query: '操作记录筛选条件无效',
  not_found: '这条操作记录已不可用',
  backend_unavailable: '操作记录查询服务暂时不可用',
  unknown: '操作记录查询失败',
};

export class TauriOperationHistoryDataSource implements OperationHistoryDataSource {
  constructor(private readonly invokeCommand: InvokeOperationHistoryCommand = invoke) {}

  async listRecords(query: OperationHistoryQuery): Promise<OperationHistoryPageResult> {
    try {
      return await this.invokeCommand<OperationHistoryPageResult>('list_operation_records', {
        query,
      });
    } catch (cause) {
      throw operationHistoryError(cause);
    }
  }

  async getRecordDetails(recordRef: string): Promise<OperationRecordDetails> {
    try {
      return await this.invokeCommand<OperationRecordDetails>('get_operation_record_details', {
        recordRef,
      });
    } catch (cause) {
      throw operationHistoryError(cause);
    }
  }
}

function operationHistoryError(cause: unknown): OperationHistoryError {
  const error = backendError(cause);
  const code =
    error instanceof RelayPulseBackendError && isOperationHistoryErrorCode(error.code)
      ? error.code
      : 'unknown';
  return new OperationHistoryError(code, messages[code]);
}

function isOperationHistoryErrorCode(value: string): value is OperationHistoryErrorCode {
  return Object.prototype.hasOwnProperty.call(messages, value) && value !== 'unknown';
}
