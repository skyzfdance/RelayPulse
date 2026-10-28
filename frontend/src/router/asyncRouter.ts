import type { RouteRecordRaw } from 'vue-router';

/** 读取 modules 目录下所有路由模块（每个文件 default 导出一个主路由记录） */
const dynamicRouteFiles = import.meta.glob('./modules/**/*.ts', {
  eager: true,
});

const moduleRecords = Object.values(dynamicRouteFiles).map(
  (module) => (module as { default: RouteRecordRaw }).default,
);

/** 异步路由表：modules 下所有主路由，按 meta.sort 升序（未定义的排最后） */
export const asyncRoutes: RouteRecordRaw[] = moduleRecords.sort((a, b) => {
  const sortA = typeof a.meta?.sort === 'number' ? a.meta.sort : Number.POSITIVE_INFINITY;
  const sortB = typeof b.meta?.sort === 'number' ? b.meta.sort : Number.POSITIVE_INFINITY;
  return sortA - sortB;
});
