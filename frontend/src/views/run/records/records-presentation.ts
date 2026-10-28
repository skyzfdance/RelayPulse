import type { ProbeResult } from '@/types/domain/records';
import type { TablePaginationConfig } from 'antdv-next';

import dayjs from 'dayjs';

export const resultLabels: Record<ProbeResult, string> = {
  success: '成功',
  slow: '偏慢',
  failure: '失败',
};
export const resultColors: Record<ProbeResult, string> = {
  success: 'var(--ant-color-success)',
  slow: 'var(--ant-color-warning)',
  failure: 'var(--ant-color-error)',
};
export const rangeOptions = [
  { value: '24h', label: '最近 24 小时' },
  { value: '7d', label: '最近 7 天' },
  { value: '30d', label: '最近 30 天' },
  { value: 'all', label: '全部保留记录' },
];
export const resultOptions = [
  { value: '', label: '全部结果' },
  ...Object.entries(resultLabels).map(([value, label]) => ({ value, label })),
];
export const USAGE_EXPLANATION =
  '仅汇总供应商响应中明确返回的 usage 字段。未上报请求不参与 Token 合计与平均值，RelayPulse 也不会根据文本长度估算用量。部分上报时，各列仅汇总已报告的值，平均每次只计算已报告总 Token 的请求。';
const numberFormat = new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 1 });
export function formatCount(value: number | null | undefined) {
  return value == null ? '—' : numberFormat.format(value);
}
export function formatDuration(value: number | null) {
  if (value === null) return '—';
  return value < 1000 ? `${value} ms` : `${(value / 1000).toFixed(2)} s`;
}
export function formatRecordTime(value: string, full = false) {
  return dayjs(value).format(full ? 'YYYY-MM-DD HH:mm:ss' : 'MM-DD HH:mm:ss');
}
export function tablePagination(
  page: number,
  pageSize: number,
  total: number,
  disabled: boolean,
): TablePaginationConfig {
  return {
    current: page,
    pageSize,
    total,
    disabled,
    showSizeChanger: true,
    pageSizeOptions: [10, 20, 50, 100],
    size: 'small',
    showTotal: (count, range) => `第 ${range[0]}–${range[1]} 条，共 ${count} 条`,
  };
}
