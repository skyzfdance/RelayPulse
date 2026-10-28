import type { RouteRecordRaw } from 'vue-router';

/**
 * IQ 测试模块：智商测试 / 历史记录 / 题库管理，外加隐藏路由「测试详情」。
 * 主路由不落地，自动转入第一个子路由。
 */
const iqTestRoutes: RouteRecordRaw = {
  path: '/iq-test',
  name: 'iq-test',
  component: () => import('@/layouts/MainLayout.vue'),
  redirect: '/iq-test/task',
  meta: { title: '测试', sort: 4, isMenu: false },
  children: [
    {
      path: 'task',
      name: 'iq-test-task',
      component: () => import('@/views/IqTestView.vue'),
      meta: {
        title: '智商测试',
        mainTitle: '智商测试',
        subTitle: '模型 IQ 评测任务',
        icon: 'ExperimentOutlined',
        isMenu: true,
      },
    },
    {
      path: 'history',
      name: 'iq-test-history',
      component: () => import('@/views/IqHistoryView.vue'),
      meta: {
        title: '历史记录',
        mainTitle: '历史记录',
        subTitle: '历次 IQ 评测结果',
        icon: 'HistoryOutlined',
        isMenu: true,
      },
    },
    {
      path: 'bank',
      name: 'iq-test-bank',
      component: () => import('@/views/IqBankView.vue'),
      meta: {
        title: '题库管理',
        mainTitle: '题库管理',
        subTitle: '评测题目的增删与维护',
        icon: 'BookOutlined',
        isMenu: true,
      },
    },
    {
      // 隐藏路由：菜单不展示，由测试页跳转进入
      path: 'detail/:id',
      name: 'iq-test-detail',
      component: () => import('@/views/IqDetailView.vue'),
      meta: {
        title: '测试详情',
        mainTitle: '测试详情',
        subTitle: '单次评测的完整报告',
        isMenu: false,
      },
    },
  ],
};

export default iqTestRoutes;
