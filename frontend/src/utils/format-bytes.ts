/** 将字节数格式化为可读大小；无效数值显示 —。 */
export function formatBytes(value: number) {
  if (!Number.isFinite(value) || value < 0) return '—';
  if (value < 1024) return `${Math.round(value)} B`;

  const units = ['KB', 'MB', 'GB', 'TB'];
  let size = value / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && size >= 1024; index += 1) {
    size /= 1024;
    unit = units[index];
  }
  const precision = size >= 10 ? 1 : 2;
  return `${size.toFixed(precision).replace(/\.0+$/, '')} ${unit}`;
}
