import type { MenuItemType } from 'antdv-next';

import { computed, h } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { asyncRoutes } from '@/router/asyncRouter';

import AppIcon from '@/components/AppIcon.vue';

/**
 * 从异步路由表生成侧边菜单。
 * 显隐规则：主路由 isMenu 为 false 时整个模块不进菜单；
 * 主路由可见时，子级再按各自 isMenu 过滤（=== true 才显示）。
 * 菜单为分组全展开形态：主路由渲染为分组标题，子项始终平铺、不可折叠。
 */
export function useMenuRoutes() {
  const route = useRoute();
  const router = useRouter();

  const menuItems = computed<MenuItemType[]>(() =>
    asyncRoutes
      .filter((main) => main.meta?.isMenu === true)
      .map((main) => {
        const children = (main.children ?? [])
          .filter((child) => child.meta?.isMenu === true)
          .map((child) => ({
            key: `${main.path}/${child.path}`,
            label: String(child.meta?.title ?? child.path),
            icon: child.meta?.icon ? h(AppIcon, { name: child.meta.icon }) : undefined,
          }));

        return {
          type: 'group' as const,
          key: main.path,
          label: String(main.meta?.title ?? main.path),
          children,
        };
      })
      .filter((main) => main.children.length > 0),
  );

  const selectedKeys = computed(() => [route.path]);

  function handleMenuClick(info: { key: string | number }) {
    void router.push(String(info.key));
  }

  return { menuItems, selectedKeys, handleMenuClick };
}
