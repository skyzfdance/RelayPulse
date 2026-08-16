import { describe, expect, it, vi } from 'vitest';
import { TauriIntelligenceLadderDataSource } from './tauri-intelligence-ladder-data-source';
import type { IntelligenceLadderSnapshot } from './intelligence-ladder-types';

const emptySnapshot: IntelligenceLadderSnapshot = {
  revision: 4,
  entries: [],
  fetchedAt: null,
  isStale: false,
  lastError: null,
  runtime: {
    state: 'idle',
    lastSuccessfulAt: null,
    nextScheduledAt: null,
  },
  settings: {
    enabled: false,
    intervalMinutes: 60,
    fetchOnAppStart: false,
    fetchOnNetworkRestore: false,
  },
  allowedIntervalMinutes: [30, 60, 180, 360],
};

describe('TauriIntelligenceLadderDataSource', () => {
  it('maps reads, refresh and settings writes to the frozen commands', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce(emptySnapshot)
      .mockResolvedValueOnce({ status: 'started' })
      .mockResolvedValueOnce({
        ...emptySnapshot,
        revision: 5,
        settings: {
          enabled: true,
          intervalMinutes: 30,
          fetchOnAppStart: true,
          fetchOnNetworkRestore: false,
        },
      });
    const dataSource = new TauriIntelligenceLadderDataSource(invoke, vi.fn(), false);

    await expect(dataSource.getSnapshot()).resolves.toEqual(emptySnapshot);
    await expect(dataSource.refresh()).resolves.toEqual({ status: 'started' });
    await expect(
      dataSource.updateAutoFetchSettings({
        enabled: true,
        intervalMinutes: 30,
        fetchOnAppStart: true,
        fetchOnNetworkRestore: false,
      }),
    ).resolves.toMatchObject({ revision: 5 });

    expect(invoke).toHaveBeenNthCalledWith(1, 'get_intelligence_ladder_snapshot', undefined);
    expect(invoke).toHaveBeenNthCalledWith(2, 'refresh_intelligence_ladder', undefined);
    expect(invoke).toHaveBeenNthCalledWith(3, 'update_intelligence_auto_fetch_settings', {
      settings: {
        enabled: true,
        intervalMinutes: 30,
        fetchOnAppStart: true,
        fetchOnNetworkRestore: false,
      },
    });
  });

  it('reloads only for intelligence-ladder change events', async () => {
    const stop = vi.fn();
    let handleEvent: ((event: { revision: number; domains: string[] }) => void) | undefined;
    const subscribe = vi.fn().mockImplementation(async (listener) => {
      handleEvent = listener;
      return stop;
    });
    const invoke = vi.fn().mockResolvedValue(undefined);
    const listener = vi.fn();
    const dataSource = new TauriIntelligenceLadderDataSource(invoke, subscribe, false);

    const unsubscribe = await dataSource.subscribeToChanges(listener);

    handleEvent?.({ revision: 1, domains: ['sites'] });
    expect(listener).not.toHaveBeenCalled();

    handleEvent?.({ revision: 2, domains: ['intelligence-ladder'] });
    expect(listener).toHaveBeenCalledOnce();

    unsubscribe();
    expect(stop).toHaveBeenCalledOnce();
  });

  it('reports network restoration for the lifetime of the production adapter', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    new TauriIntelligenceLadderDataSource(invoke, vi.fn(), true);

    window.dispatchEvent(new Event('online'));

    expect(invoke).toHaveBeenCalledWith('notify_intelligence_network_restored', undefined);
  });
});
