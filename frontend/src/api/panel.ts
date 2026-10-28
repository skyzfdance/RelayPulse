/** 小面板的数据与动作入口，统一由 callBackend 选择 Tauri 或浏览器 mock。 */
import type { PanelSnapshot, PanelStateChanged } from '@/types/domain/panel';
import type { ThemeMode } from '@/types/theme';

import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { callBackend } from './call-backend';
import { subscribe } from './events';

export const PANEL_STATE_CHANGED = 'panel:state-changed';

/** 首次读取失败由面板内的错误状态和重试入口承接。 */
export function fetchPanelSnapshot(): Promise<PanelSnapshot> {
  return callBackend('get_panel_snapshot', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}

/** 执行刷新并返回当前快照，失败由统一调用层提示。 */
export function refreshPanel(): Promise<PanelSnapshot> {
  return callBackend('refresh_panel', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}

/** 同步独立面板尺寸与原生主题，返回是否配置系统模糊；其他环境使用实色底。 */
export async function syncPanelWindow(height: number, theme: ThemeMode): Promise<boolean> {
  if (!isTauri() || getCurrentWindow().label !== 'status-panel') return false;
  return (
    (await callBackend<boolean>(
      'sync_panel_window',
      { height, theme },
      {
        successFeedback: 'none',
        failFeedback: 'none',
      },
    )) === true
  );
}

/** 只处理明确的设置点击；浏览器预览由页面宿主承接，不创建浏览器产品跳转。 */
export async function openPanelSettings(): Promise<void> {
  if (!isTauri() || getCurrentWindow().label !== 'status-panel') return;
  return callBackend('open_panel_settings', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}

/** 查看完整站点与模型状态；窗口显示、导航和面板关闭统一交给桌面宿主。 */
export async function openPanelSites(): Promise<void> {
  if (!isTauri() || getCurrentWindow().label !== 'status-panel') return;
  return callBackend('open_panel_sites', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}

/** 隐藏时保留订阅；重新显示/聚焦时补读。调用者在销毁时释放全部监听。 */
export async function subscribePanelUpdates(
  changed: (event: PanelStateChanged) => void,
  shown: () => void,
): Promise<() => void> {
  const label = isTauri() ? getCurrentWindow().label : undefined;
  const options = label ? { target: { kind: 'WebviewWindow' as const, label } } : undefined;
  const stopChanges = await subscribe<PanelStateChanged>(PANEL_STATE_CHANGED, changed, options);
  let stopFocus: (() => void) | undefined;
  try {
    if (label) {
      stopFocus = await subscribe('tauri://focus', shown, { target: { kind: 'Window', label } });
    }
  } catch (cause) {
    stopChanges();
    throw cause;
  }
  const visibility = () => {
    if (document.visibilityState === 'visible') shown();
  };
  document.addEventListener('visibilitychange', visibility);
  window.addEventListener('pageshow', shown);
  return () => {
    stopChanges();
    stopFocus?.();
    document.removeEventListener('visibilitychange', visibility);
    window.removeEventListener('pageshow', shown);
  };
}
