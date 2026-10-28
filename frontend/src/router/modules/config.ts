import type { RouteRecordRaw } from 'vue-router';

/**
 * 配置模块：站点管理 / 通用设置 / 消息通知。
 * 主路由不落地，自动转入第一个子路由。
 */
const configRoutes: RouteRecordRaw = {
  path: '/config',
  name: 'config',
  component: () => import('@/layouts/MainLayout.vue'),
  redirect: '/config/sites',
  meta: { title: '配置', sort: 1, isMenu: true },
  children: [
    {
      path: 'sites',
      name: 'sites',
      component: () => import('@/views/config/sites/SitesView.vue'),
      meta: {
        title: '站点管理',
        mainTitle: '站点管理',
        subTitle: '配置多个模型站点，并通过拖动顺序决定故障时的优先级',
        icon: 'AimOutlined',
        isMenu: true,
      },
    },
    {
      path: 'general',
      name: 'general',
      component: () => import('@/views/config/general/GeneralView.vue'),
      meta: {
        title: '通用设置',
        mainTitle: '通用设置',
        subTitle: '控制 RelayPulse 的系统行为、外观与本地数据',
        icon: 'SettingOutlined',
        isMenu: true,
      },
    },
    {
      path: 'notifications',
      name: 'notifications',
      component: () => import('@/views/config/notifications/NotificationsView.vue'),
      meta: {
        title: '消息通知',
        mainTitle: '消息通知',
        subTitle: '管理通知渠道和需要接收的事件',
        icon: 'BellOutlined',
        isMenu: true,
      },
    },
  ],
};

export default configRoutes;
