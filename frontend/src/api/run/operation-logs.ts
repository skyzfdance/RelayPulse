import type {
  OperationLogDetail,
  OperationLogPage,
  OperationLogQuery,
} from '@/types/domain/operation-logs';

import { callBackend } from '../call-backend';

const READ_OPTIONS = { successFeedback: 'none', failFeedback: 'none' } as const;

export function fetchOperationLogs(query: OperationLogQuery): Promise<OperationLogPage> {
  return callBackend(
    'get_operation_logs',
    {
      query: {
        startDate: query.startDate,
        endDate: query.endDate,
        source: [...query.source],
        result: [...query.result],
        actions: [...query.actions],
        page: query.page,
        pageSize: query.pageSize,
      },
    },
    READ_OPTIONS,
  );
}

export function fetchOperationLogDetail(logId: string): Promise<OperationLogDetail> {
  return callBackend('get_operation_log_detail', { logId }, READ_OPTIONS);
}
