/** 桌面宿主向已有主窗口发送导航请求；浏览器预览不参与窗口通信。 */
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { subscribe } from './events';

export type MainWindowRoute = '/config/general' | '/config/sites' | '/run/records';

export async function subscribeMainWindowNavigation(
  navigate: (route: MainWindowRoute) => void,
): Promise<() => void> {
  if (!isTauri() || getCurrentWindow().label !== 'main') return () => {};
  return subscribe<unknown>(
    'main-window:navigate',
    (route) => {
      if (route === '/config/general' || route === '/config/sites' || route === '/run/records')
        navigate(route);
    },
    { target: { kind: 'WebviewWindow', label: 'main' } },
  );
}
