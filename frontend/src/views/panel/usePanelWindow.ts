import type { ThemeMode } from '@/types/theme';
import type { Ref } from 'vue';

import { onBeforeUnmount, ref, watch } from 'vue';

import { syncPanelWindow } from '@/api/panel';
import { reportFrontendError } from '@/utils/diagnostics';

interface Presentation {
  height: number;
  theme: ThemeMode;
}

/** 同步实际尺寸与原生主题；首次有内容后才允许宿主显示。 */
export function usePanelWindow(content: Ref<HTMLElement | null>, theme: Ref<ThemeMode>) {
  // 原生模糊未经宿主确认前保持实色，避免透明 WebView 直接透出桌面文字。
  const nativeBackdrop = ref(false);
  let disposed = false;
  let syncing = false;
  let pending: Presentation | undefined;
  let synced: Presentation | undefined;
  let measure: (() => void) | undefined;

  async function synchronize() {
    if (syncing) return;
    syncing = true;
    try {
      while (pending !== undefined && !disposed) {
        const next = pending;
        pending = undefined;
        if (next.height === synced?.height && next.theme === synced.theme) continue;
        try {
          const backdrop = await syncPanelWindow(next.height, next.theme);
          if (!disposed) nativeBackdrop.value = backdrop;
          synced = next;
        } catch (cause) {
          reportFrontendError('background-task', cause);
        }
      }
    } finally {
      syncing = false;
    }
  }

  watch(
    content,
    (element, _, cleanup) => {
      if (!element) return;
      const measureContent = () => {
        if (disposed) return;
        const height = Math.ceil(element.getBoundingClientRect().height);
        if (height <= 0) return;
        pending = { height, theme: theme.value };
        void synchronize();
      };
      measure = measureContent;
      const observer = new ResizeObserver(measureContent);
      observer.observe(element);
      measureContent();
      cleanup(() => {
        observer.disconnect();
        measure = undefined;
      });
    },
    { flush: 'post' },
  );

  watch(theme, () => measure?.(), { flush: 'post' });

  onBeforeUnmount(() => {
    disposed = true;
    pending = undefined;
  });

  return { nativeBackdrop };
}
