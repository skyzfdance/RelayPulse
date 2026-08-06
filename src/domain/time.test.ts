import { describe, expect, it } from 'vitest';
import { formatBytes, formatLocalTime, formatLocalTimestamp } from './time';

describe('local time formatting', () => {
  it('converts an ISO timestamp using the operating-system timezone', () => {
    const value = '2026-08-04T03:27:52Z';
    const local = new Date(value);
    const time = [local.getHours(), local.getMinutes(), local.getSeconds()]
      .map((part) => String(part).padStart(2, '0'))
      .join(':');
    const date = [local.getMonth() + 1, local.getDate()]
      .map((part) => String(part).padStart(2, '0'))
      .join('-');

    expect(formatLocalTime(value)).toBe(time);
    expect(formatLocalTimestamp(value)).toBe(`${date} ${time}`);
  });

  it('uses a placeholder for an invalid timestamp', () => {
    expect(formatLocalTime('invalid')).toBe('—');
    expect(formatLocalTimestamp('invalid')).toBe('—');
  });
});

describe('byte formatting', () => {
  it('uses compact binary units for diagnostic storage', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1536)).toBe('1.50 KB');
    expect(formatBytes(10 * 1024 * 1024)).toBe('10 MB');
  });

  it('rejects invalid sizes', () => {
    expect(formatBytes(-1)).toBe('—');
    expect(formatBytes(Number.NaN)).toBe('—');
  });
});
