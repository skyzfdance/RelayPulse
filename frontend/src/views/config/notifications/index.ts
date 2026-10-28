/**
 * config/notifications 对外唯一出口。路由不在此（留在 router/modules/config.ts），
 * 这里只暴露跨特性可能复用的东西——当前为页面组件本身。
 */
export { default as NotificationsView } from './NotificationsView.vue';
