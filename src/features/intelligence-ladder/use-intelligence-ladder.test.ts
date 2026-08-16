import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useIntelligenceLadder } from './use-intelligence-ladder';
import type {
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
} from './intelligence-ladder-types';

function snapshot(revision: number): IntelligenceLadderSnapshot {
  return {
    revision,
    entries: [],
    fetchedAt: null,
    isStale: false,
    lastError: null,
    runtime: { state: 'idle', lastSuccessfulAt: null, nextScheduledAt: null },
    settings: {
      enabled: false,
      intervalMinutes: 60,
      fetchOnAppStart: false,
      fetchOnNetworkRestore: false,
    },
    allowedIntervalMinutes: [30, 60, 180, 360],
  };
}

describe('useIntelligenceLadder', () => {
  it('subscribes before the first read and ignores a late older revision', async () => {
    let resolveFirstRead: ((value: IntelligenceLadderSnapshot) => void) | undefined;
    let resolveLateRead: ((value: IntelligenceLadderSnapshot) => void) | undefined;
    let subscribed = false;
    let emitChange: (() => void) | undefined;
    const order: string[] = [];
    const stop = vi.fn();
    const dataSource: IntelligenceLadderDataSource = {
      getSnapshot: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<IntelligenceLadderSnapshot>((resolve) => {
              order.push('read');
              resolveFirstRead = resolve;
            }),
        )
        .mockImplementationOnce(
          () =>
            new Promise<IntelligenceLadderSnapshot>((resolve) => {
              order.push('read');
              resolveLateRead = resolve;
            }),
        ),
      refresh: vi.fn(),
      updateAutoFetchSettings: vi.fn(),
      subscribeToChanges: vi.fn(async (listener) => {
        order.push('subscribe');
        subscribed = true;
        emitChange = listener;
        return stop;
      }),
    };

    const { result, unmount } = renderHook(() => useIntelligenceLadder(dataSource));
    expect(subscribed).toBe(true);
    await act(async () => undefined);
    expect(order).toEqual(['subscribe', 'read']);

    await act(async () => {
      emitChange?.();
      resolveLateRead?.(snapshot(5));
      await Promise.resolve();
    });
    expect(result.current.snapshot?.revision).toBe(5);

    await act(async () => {
      resolveFirstRead?.(snapshot(4));
      await Promise.resolve();
    });
    expect(result.current.snapshot?.revision).toBe(5);

    unmount();
    expect(stop).toHaveBeenCalledOnce();
  });
});
