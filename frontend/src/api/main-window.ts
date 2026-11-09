/** 桌面宿主向已有主窗口发送导航请求；浏览器预览不参与窗口通信。 */
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { subscribe } from './events';

export type MainWindowRoute = '/config/general' | '/config/sites' | '/run/records';
export type MainWindowNavigationTarget =
  MainWindowRoute | { path: '/iq-test/history'; query: { runId: string } };

function navigationTarget(value: unknown): MainWindowNavigationTarget | undefined {
  if (value === '/config/general' || value === '/config/sites' || value === '/run/records')
    return value;
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  const target = value as Record<string, unknown>;
  if (
    target.path !== '/iq-test/history' ||
    Object.keys(target).length !== 2 ||
    !Object.prototype.hasOwnProperty.call(target, 'path') ||
    !Object.prototype.hasOwnProperty.call(target, 'query') ||
    !target.query ||
    typeof target.query !== 'object' ||
    Array.isArray(target.query)
  )
    return;
  const query = target.query as Record<string, unknown>;
  if (
    Object.keys(query).length !== 1 ||
    !Object.prototype.hasOwnProperty.call(query, 'runId') ||
    typeof query.runId !== 'string' ||
    !/^[a-zA-Z0-9_-]+$/u.test(query.runId)
  )
    return;
  return { path: '/iq-test/history', query: { runId: query.runId } };
}

export async function subscribeMainWindowNavigation(
  navigate: (target: MainWindowNavigationTarget) => void,
): Promise<() => void> {
  if (!isTauri() || getCurrentWindow().label !== 'main') return () => {};
  return subscribe<unknown>(
    'main-window:navigate',
    (payload) => {
      const target = navigationTarget(payload);
      if (target) navigate(target);
    },
    { target: { kind: 'WebviewWindow', label: 'main' } },
  );
}
