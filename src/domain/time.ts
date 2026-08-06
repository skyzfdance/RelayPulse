function parseTimestamp(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function twoDigits(value: number) {
  return String(value).padStart(2, '0');
}

export function formatLocalTimestamp(value: string) {
  const date = parseTimestamp(value);
  if (!date) return '—';

  return `${twoDigits(date.getMonth() + 1)}-${twoDigits(date.getDate())} ${formatLocalTime(value)}`;
}

export function formatLocalTime(value: string) {
  const date = parseTimestamp(value);
  if (!date) return '—';

  return `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}:${twoDigits(date.getSeconds())}`;
}

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
