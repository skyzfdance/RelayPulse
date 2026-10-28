import 'vue-router';

declare module 'vue-router' {
  interface RouteMeta {
    /** 菜单显示名字 */
    title?: string;
    /** 路由落地页面标题 */
    mainTitle?: string;
    /** 路由落地页面子标题 */
    subTitle?: string;
    /** 菜单图标（@antdv-next/icons 组件名，布局层解析） */
    icon?: string;
    /** 是否在菜单栏显示 */
    isMenu?: boolean;
    /** 主路由排序，数字越小越靠前 */
    sort?: number;
  }
}
