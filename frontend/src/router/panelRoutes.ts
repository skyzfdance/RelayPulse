import type { RouteRecordRaw } from 'vue-router';

/** 独立窗口路由，不经过 MainLayout，也不参与侧边菜单。 */
export const panelRoutes: RouteRecordRaw[] = [
  {
    path: '/panel',
    name: 'panel',
    component: () => import('@/views/panel/PanelView.vue'),
    meta: { isMenu: false, mainTitle: '小面板' },
  },
];
