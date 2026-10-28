import type { RouteRecordRaw } from 'vue-router';

/**
 * 情报模块：智力天梯。
 * 主路由不落地，自动转入第一个子路由。
 */
const intelRoutes: RouteRecordRaw = {
  path: '/intel',
  name: 'intel',
  component: () => import('@/layouts/MainLayout.vue'),
  redirect: '/intel/intelligence-ladder',
  meta: { title: '情报', sort: 3, isMenu: false },
  children: [
    {
      path: 'intelligence-ladder',
      name: 'intelligence-ladder',
      component: () => import('@/views/IntelligenceLadderView.vue'),
      meta: {
        title: '智力天梯',
        mainTitle: '智力天梯',
        subTitle: '查看模型智力效率排名',
        icon: 'TrophyOutlined',
        isMenu: true,
      },
    },
  ],
};

export default intelRoutes;
