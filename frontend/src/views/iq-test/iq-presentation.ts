export const iqStatusLabels: Record<string, string> = {
  pending: '待开始',
  running: '运行中',
  completed: '已完成',
  error: '失败',
  timeout: '超时',
  aborted: '已中止',
};

export const iqStatusColors: Record<string, string> = {
  pending: 'default',
  running: 'processing',
  completed: 'success',
  error: 'error',
  timeout: 'warning',
  aborted: 'warning',
};

export function isIqEntryTerminal(status: string) {
  return ['completed', 'error', 'timeout', 'aborted'].includes(status);
}

export function formatIqTime(time: number | null | undefined) {
  if (time == null) return '—';
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(time);
}

export function formatIqDuration(ms: number | null | undefined) {
  if (ms == null) return '—';
  if (ms < 1000) return `${Math.round(ms)} 毫秒`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)} 秒`;
  return `${Math.floor(ms / 60000)} 分 ${Math.floor((ms % 60000) / 1000)} 秒`;
}

const iqPresetLabels: Record<string, string> = {
  minimal: '最低',
  low: '低',
  medium: '中',
  high: '高',
  xhigh: '超高',
  max: '最大',
  off: '关闭',
  custom: '自定义',
};

export function formatIqStrength(
  participant: { preset: string; parameters: Record<string, unknown> } | undefined,
) {
  if (!participant) return '—';
  if (participant.preset === 'custom')
    return `自定义 · ${Object.keys(participant.parameters).length} 项参数`;
  return iqPresetLabels[participant.preset] ?? participant.preset;
}
