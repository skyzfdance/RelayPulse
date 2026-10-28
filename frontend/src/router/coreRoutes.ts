import type { RouteRecordRaw } from 'vue-router';

import NotFoundView from '@/views/error/NotFoundView.vue';
import ServiceUnavailableView from '@/views/error/ServiceUnavailableView.vue';

/**
 * 基础路由：错误页。同步导入，保证任何情况下立即可用。
 * meta 只需 isMenu: false（不进菜单），其余字段对错误页没有意义。
 * 注意：catch-all 兜底必须排在 children 最后（vue-router 4 按特异性打分，
 * 兜底路由天然最低分，放最后是为了结构上一眼可见）。
 */
export const coreRoutes: RouteRecordRaw[] = [
  {
    path: '/503',
    name: 'service-unavailable',
    component: ServiceUnavailableView,
    meta: { isMenu: false },
  },
  {
    path: '/:pathMatch(.*)*',
    name: 'not-found',
    component: NotFoundView,
    meta: { isMenu: false },
  },
];
