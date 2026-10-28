import type { PanelModelStatus } from '@/types/domain/panel';

export const panelModelLabels: Record<PanelModelStatus, string> = {
  healthy: '正常',
  unavailable: '不可用',
  slow: '偏慢',
  pending: '待确认',
};

export function formatPanelDuration(value: number | null): string {
  if (value == null) return '—';
  return value < 1000 ? `${value} ms` : `${(value / 1000).toFixed(2)} s`;
}

const timeFormats = {
  minute: new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }),
  second: new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  }),
};

export function formatPanelTime(value: string, precision: 'minute' | 'second' = 'minute'): string {
  return timeFormats[precision].format(new Date(value));
}

export function panelTimeTitle(value: string): string {
  return new Date(value).toLocaleString('zh-CN', { hour12: false });
}
