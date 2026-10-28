import type { MainWindowRoute } from '@/api/main-window';
import type { InjectionKey } from 'vue';

import { inject, onScopeDispose, provide } from 'vue';
import { useRouter } from 'vue-router';

import { subscribeMainWindowNavigation } from '@/api/main-window';
import { reportFrontendError } from '@/utils/diagnostics';

import { useFeedback } from './useFeedback';

type Refresh = () => unknown;
const refreshKey: InjectionKey<Map<MainWindowRoute, Refresh>> = Symbol('main-window-refresh');

/** 在应用根部建立一次监听；路由离开保护仍由 Vue Router 执行。 */
export function provideMainWindowNavigation() {
  const router = useRouter();
  const refreshers = new Map<MainWindowRoute, Refresh>();
  provide(refreshKey, refreshers);
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  function failed(cause: unknown) {
    if (disposed) return;
    reportFrontendError('background-task', cause);
    useFeedback().error('无法打开目标页面，请重试。', 'message');
  }
  async function navigate(route: MainWindowRoute) {
    await router.isReady();
    if (disposed) return;
    if (router.currentRoute.value.path === route) await refreshers.get(route)?.();
    else await router.push(route);
  }
  void subscribeMainWindowNavigation((route) => {
    void navigate(route).catch(failed);
  }).then((stop) => {
    if (disposed) stop();
    else unsubscribe = stop;
  }, failed);
  onScopeDispose(() => {
    disposed = true;
    unsubscribe?.();
    refreshers.clear();
  });
}

/** 页面已打开时重新取数，不销毁页面、筛选条件或表单草稿。 */
export function onMainWindowRefresh(route: MainWindowRoute, refresh: Refresh) {
  const refreshers = inject(refreshKey, undefined);
  refreshers?.set(route, refresh);
  onScopeDispose(() => {
    if (refreshers?.get(route) === refresh) refreshers.delete(route);
  });
}
