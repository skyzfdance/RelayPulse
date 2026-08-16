import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const eventListeners = vi.hoisted(() => new Map<string, () => void>());
const stopListening = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => true,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, listener: () => void) => {
    eventListeners.set(event, listener);
    return stopListening;
  }),
}));

import {
  STATUS_PANEL_CLOSING_EVENT,
  STATUS_PANEL_OPENING_EVENT,
  useTrayPanelMotion,
} from './tray-panel-motion';

describe('useTrayPanelMotion', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    eventListeners.clear();
    stopListening.mockClear();
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) =>
      window.setTimeout(() => callback(performance.now()), 16),
    );
    vi.stubGlobal('cancelAnimationFrame', (frame: number) => window.clearTimeout(frame));
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('opens on mount, closes on request, and can be opened again', async () => {
    const { result, unmount } = renderHook(() => useTrayPanelMotion());

    expect(result.current).toBe('closed');
    await act(async () => undefined);
    await act(async () => {
      vi.advanceTimersByTime(32);
    });
    expect(result.current).toBe('open');

    await act(async () => {
      eventListeners.get(STATUS_PANEL_CLOSING_EVENT)?.();
    });
    expect(result.current).toBe('closed');

    await act(async () => {
      eventListeners.get(STATUS_PANEL_OPENING_EVENT)?.();
    });
    await act(async () => {
      vi.advanceTimersByTime(32);
    });
    expect(result.current).toBe('open');

    unmount();
    expect(stopListening).toHaveBeenCalledTimes(2);
  });
});
