import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';

export const STATUS_PANEL_OPENING_EVENT = 'relaypulse://status-panel-opening';
export const STATUS_PANEL_CLOSING_EVENT = 'relaypulse://status-panel-closing';

export type TrayPanelMotion = 'closed' | 'open';

export function useTrayPanelMotion(): TrayPanelMotion {
  const [motion, setMotion] = useState<TrayPanelMotion>('closed');

  useEffect(() => {
    let disposed = false;
    let frames: number[] = [];
    let unlisten: Array<() => void> = [];

    function cancelFrames() {
      frames.forEach((frame) => window.cancelAnimationFrame(frame));
      frames = [];
    }

    function openPanel() {
      cancelFrames();
      setMotion('closed');
      frames.push(
        window.requestAnimationFrame(() => {
          frames.push(
            window.requestAnimationFrame(() => {
              if (!disposed) setMotion('open');
            }),
          );
        }),
      );
    }

    function closePanel() {
      cancelFrames();
      setMotion('closed');
    }

    openPanel();
    if (!isTauri()) {
      return () => {
        disposed = true;
        cancelFrames();
      };
    }

    void Promise.all([
      listen(STATUS_PANEL_OPENING_EVENT, openPanel),
      listen(STATUS_PANEL_CLOSING_EVENT, closePanel),
    ]).then((listeners) => {
      if (disposed) {
        listeners.forEach((stop) => stop());
      } else {
        unlisten = listeners;
      }
    });

    return () => {
      disposed = true;
      cancelFrames();
      unlisten.forEach((stop) => stop());
    };
  }, []);

  return motion;
}
