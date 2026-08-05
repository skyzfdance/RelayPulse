import { describe, expect, it } from 'vitest';
import { defaultViewForWindow, isViewAllowedForWindow, parseView } from './view';

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

describe('window view isolation', () => {
  it('keeps tray views out of the main window', () => {
    expect(isViewAllowedForWindow('main', 'general')).toBe(true);
    expect(isViewAllowedForWindow('main', 'tray-healthy')).toBe(false);
    expect(defaultViewForWindow('main')).toBe('sites');
  });

  it('only permits tray views in the status panel', () => {
    expect(isViewAllowedForWindow('status-panel', 'tray-alerts')).toBe(true);
    expect(isViewAllowedForWindow('status-panel', 'records')).toBe(false);
    expect(defaultViewForWindow('status-panel')).toBe('tray-healthy');
  });
});
