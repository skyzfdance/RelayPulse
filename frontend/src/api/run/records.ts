/** 页面业务数据的唯一入口；浏览器已接 Mock，真实后端尚待按此契约接入。 */
import type {
  OpenProbeRecordLogResult,
  ProbeRecordDetail,
  ProbeRecordListItem,
  ProbeRecordOptions,
  ProbeRecordRequestContent,
  ProbeRecordResponseContent,
  ProbeRecordsQuery,
  RecordsPage,
  TokenStatsPage,
  TokenStatsQuery,
} from '@/types/domain/records';

import { callBackend } from '../call-backend';

// 读取失败由当前视图内的错误状态和重试入口承接，避免重复弹窗。
const READ_OPTIONS = { successFeedback: 'none', failFeedback: 'none' } as const;

export function fetchProbeRecordOptions(): Promise<ProbeRecordOptions> {
  return callBackend('get_probe_record_options', undefined, READ_OPTIONS);
}

export function fetchProbeRecords(
  query: ProbeRecordsQuery,
): Promise<RecordsPage<ProbeRecordListItem>> {
  return callBackend('get_probe_records', { query: { ...query } }, READ_OPTIONS);
}

export function fetchProbeTokenStats(query: TokenStatsQuery): Promise<TokenStatsPage> {
  return callBackend('get_probe_token_stats', { query: { ...query } }, READ_OPTIONS);
}

export function fetchProbeRecordDetail(recordId: string): Promise<ProbeRecordDetail> {
  return callBackend('get_probe_record_detail', { recordId }, READ_OPTIONS);
}

/** 仅在展开请求详情时读取已保存的脱敏内容。 */
export function fetchProbeRecordRequest(recordId: string): Promise<ProbeRecordRequestContent> {
  return callBackend('get_probe_record_request', { recordId }, READ_OPTIONS);
}

/** 按需读取响应元数据、日志路径及失败时的响应体，不加载正常响应的合并流。 */
export function fetchProbeRecordResponse(recordId: string): Promise<ProbeRecordResponseContent> {
  return callBackend('get_probe_record_response', { recordId }, READ_OPTIONS);
}

/** 由后端按正式记录 ID 打开日志所属目录；反馈隔离已关闭详情的旧结果。 */
export function openProbeRecordLogDirectory(recordId: string): Promise<OpenProbeRecordLogResult> {
  return callBackend('open_probe_record_log_directory', { recordId }, READ_OPTIONS);
}

export function deleteProbeRecord(recordId: string): Promise<void> {
  return callBackend<void>(
    'delete_probe_record',
    { recordId },
    {
      successText: '探测记录已删除',
      failFeedback: 'message',
    },
  );
}

/** 清理全部站点、全部保留时间范围的正式记录，不携带当前列表筛选。 */
export function clearProbeRecords(): Promise<void> {
  return callBackend<void>('clear_probe_records', undefined, {
    successText: '探测记录已清理',
    failFeedback: 'message',
  });
}
