import type { Router } from 'vue-router';

import { useAppInfo } from '@/composables/useAppInfo';

/**
 * 路由守卫。当前职责：按落地页 meta 同步文档标题；
 * 后续的鉴权、窗口限制（托盘窗口）等拦截逻辑都挂在这里。
 */
export function setupRouterGuard(router: Router) {
  const { appName } = useAppInfo();

  router.beforeEach((to) => {
    document.title = to.meta.mainTitle ? `${String(to.meta.mainTitle)} · ${appName}` : appName;
    return true;
  });
}
