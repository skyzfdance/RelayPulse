import type { PanelSnapshot } from '@/types/domain/panel';

import { onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';

import { fetchPanelSnapshot, refreshPanel, subscribePanelUpdates } from '@/api/panel';
import { useFeedback } from '@/composables/useFeedback';

/** 订阅先于初读；通知合并重读，刷新返回与事件重读共用版本检查。 */
export function usePanelSnapshot() {
  const snapshot = shallowRef<PanelSnapshot | null>(null);
  const refreshing = ref(false);
  const loadError = ref('');
  let disposed = false;
  let minimumRevision = 0;
  let dirty = false;
  let reading: Promise<void> | undefined;
  let connecting: Promise<void> | undefined;
  let unlisten: (() => void) | undefined;

  function accept(next: PanelSnapshot) {
    if (disposed || next.revision < minimumRevision) return;
    if (snapshot.value && next.revision < snapshot.value.revision) return;
    snapshot.value = next;
    loadError.value = '';
  }

  function read(): Promise<void> {
    if (disposed) return Promise.resolve();
    dirty = true;
    reading ??= (async () => {
      while (dirty && !disposed) {
        dirty = false;
        const requestedRevision = minimumRevision;
        try {
          accept(await fetchPanelSnapshot());
        } catch (cause) {
          if (!disposed && !dirty && requestedRevision === minimumRevision) {
            const message = cause instanceof Error ? cause.message : '读取面板状态失败，请重试。';
            if (snapshot.value) useFeedback().error(message, 'message');
            else loadError.value = message;
          }
        }
      }
    })().finally(() => {
      reading = undefined;
      if (dirty && !disposed) void read();
    });
    return reading;
  }

  async function loadSnapshot() {
    if (disposed) return;
    loadError.value = '';
    if (!unlisten) {
      connecting ??= (async () => {
        try {
          const stop = await subscribePanelUpdates(
            (event) => {
              if (disposed) return;
              minimumRevision = Math.max(minimumRevision, event.revision);
              dirty = true;
              if (unlisten) void read();
            },
            () => {
              if (disposed) return;
              dirty = true;
              if (unlisten) void read();
            },
          );
          if (disposed) stop();
          else unlisten = stop;
        } finally {
          connecting = undefined;
        }
      })();
      try {
        await connecting;
      } catch (cause) {
        if (!disposed)
          loadError.value = cause instanceof Error ? cause.message : '订阅面板状态失败，请重试。';
        return;
      }
    }
    await read();
  }

  async function refresh() {
    if (!snapshot.value || refreshing.value || snapshot.value.activeProbe || disposed) return;
    refreshing.value = true;
    try {
      accept(await refreshPanel());
    } catch {
      // 动作错误由统一 API 提示；补读已提交的部分结果及真正的 activeProbe。
      await read();
    } finally {
      if (!disposed) refreshing.value = false;
    }
  }

  onMounted(loadSnapshot);
  onBeforeUnmount(() => {
    disposed = true;
    unlisten?.();
    unlisten = undefined;
  });
  return { snapshot, refreshing, loadError, loadSnapshot, refresh };
}
