import { describe, expect, it } from 'vitest';
import { formatLocalTime, formatLocalTimestamp } from './time';

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
