/**
 * config/general 对外唯一出口。路由不在此（留在 router/modules/config.ts），
 * 这里只暴露页面组件本身。
 */
export { default as GeneralView } from './GeneralView.vue';
