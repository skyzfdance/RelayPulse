import { describe, expect, it } from 'vitest';
import { isNewerRevision } from './revision';

describe('isNewerRevision', () => {
  it('accepts only newer state events', () => {
    expect(isNewerRevision(null, 1)).toBe(true);
    expect(isNewerRevision(5, 6)).toBe(true);
    expect(isNewerRevision(5, 5)).toBe(false);
    expect(isNewerRevision(5, 4)).toBe(false);
  });
});
