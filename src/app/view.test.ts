import { describe, expect, it } from 'vitest';
import { parseView } from './view';

describe('parseView', () => {
  it.each([
    'sites',
    'add-site',
    'site-advanced',
    'model-settings',
    'general',
    'records',
    'delete-confirm',
    'empty',
    'tray-healthy',
    'tray-alerts',
  ] as const)('accepts %s', (view) => {
    expect(parseView(`?view=${view}`)).toBe(view);
  });

  it.each(['', '?view=unknown', '?other=value'])('falls back to sites for %s', (search) => {
    expect(parseView(search)).toBe('sites');
  });
});
