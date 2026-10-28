import type { RouteRecordRaw } from 'vue-router';

/**
 * 运行模块：探测记录 / 操作记录。
 * 主路由不落地，自动转入第一个子路由。
 */
const runRoutes: RouteRecordRaw = {
  path: '/run',
  name: 'run',
  component: () => import('@/layouts/MainLayout.vue'),
  redirect: '/run/records',
  meta: { title: '运行', sort: 2, isMenu: true },
  children: [
    {
      path: 'records',
      name: 'records',
      component: () => import('@/views/run/records/RecordsView.vue'),
      meta: {
        title: '探测记录',
        mainTitle: '探测记录',
        subTitle: '查看模型可用性、响应耗时和供应商上报的 Token 用量',
        icon: 'HistoryOutlined',
        isMenu: true,
      },
    },
    {
      path: 'operation-history',
      name: 'operation-history',
      component: () => import('@/views/OperationHistoryView.vue'),
      meta: {
        title: '操作记录',
        mainTitle: '操作记录',
        subTitle: '查看应用运行、配置变更、数据采集和诊断维护记录',
        icon: 'ProfileOutlined',
        isMenu: false,
      },
    },
  ],
};

export default runRoutes;
