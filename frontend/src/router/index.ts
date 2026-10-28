import type { RouteRecordRaw } from 'vue-router';

import { createRouter, createWebHistory } from 'vue-router';

import { asyncRoutes } from './asyncRouter';
import { coreRoutes } from './coreRoutes';
import { setupRouterGuard } from './guard';
import { panelRoutes } from './panelRoutes';

/** 路由统一出口：根记录聚合 children = 异步主路由 + 基础路由（catch-all 在最后） */
export const routes: RouteRecordRaw[] = [
  {
    path: '/',
    redirect: '/config',
    meta: { isMenu: false },
    children: [...asyncRoutes, ...panelRoutes, ...coreRoutes],
  },
];

export const router = createRouter({
  history: createWebHistory(),
  routes,
});

setupRouterGuard(router);

router.afterEach((to) => {
  if (to.name === 'panel') document.documentElement.dataset.surface = 'panel';
  else delete document.documentElement.dataset.surface;
});
