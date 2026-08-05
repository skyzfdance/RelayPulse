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
