/** 页面业务数据的唯一入口；Tauri 读取真实后端，浏览器预览使用 Mock。 */
import type {
  OpenProbeRecordLogResult,
  ProbeRecordAssociationLookup,
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

/** 按真实 ID 集合定位；分批读取大型任务的全部记录，保持每次命令的资源上限。 */
export async function fetchProbeRecordsByIds(
  recordIds: string[],
): Promise<ProbeRecordAssociationLookup> {
  const ids = [...new Set(recordIds)];
  const items: ProbeRecordListItem[] = [];
  const missingIds: string[] = [];
  // 空参数也交给后端校验；不能把无效请求伪装成成功的空集合。
  for (let offset = 0; offset < Math.max(1, ids.length); offset += 100) {
    const result = await callBackend<ProbeRecordAssociationLookup>(
      'get_probe_records_by_ids',
      { recordIds: ids.slice(offset, offset + 100) },
      READ_OPTIONS,
    );
    items.push(...result.items);
    missingIds.push(...result.missingIds);
  }
  return { items, missingIds };
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
