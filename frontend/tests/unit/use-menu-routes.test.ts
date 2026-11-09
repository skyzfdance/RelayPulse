import type { RouteRecordRaw } from 'vue-router';

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import { defineComponent, h, nextTick } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';

import { useMenuRoutes } from '@/composables/useMenuRoutes';

const MenuRouteProbe = defineComponent({
  setup() {
    const { selectedKeys } = useMenuRoutes();
    return () => h('output', selectedKeys.value[0]);
  },
});

describe('useMenuRoutes 高亮目标', () => {
  it('默认高亮自身，字符串目标与空目标均按 route meta 解析', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/self', component: MenuRouteProbe },
        { path: '/self-empty', component: MenuRouteProbe, meta: { highlightRoute: '' } },
        {
          path: '/string-target',
          component: MenuRouteProbe,
          meta: { highlightRoute: '/menu/target' },
        },
        { path: '/menu/target', component: MenuRouteProbe },
        {
          path: '/dynamic/:id',
          component: MenuRouteProbe,
          meta: {
            highlightRoute: (route) => (route.query.from === 'history' ? '/menu/history' : ''),
          },
        },
      ] satisfies RouteRecordRaw[],
    });

    await router.push('/self');
    const wrapper = mount(MenuRouteProbe, { global: { plugins: [router] } });
    expect(wrapper.text()).toBe('/self');

    await router.push('/self-empty');
    await nextTick();
    expect(wrapper.text()).toBe('/self-empty');

    await router.push('/string-target');
    await nextTick();
    expect(wrapper.text()).toBe('/menu/target');

    await router.push('/dynamic/entry?from=history');
    await nextTick();
    expect(wrapper.text()).toBe('/menu/history');

    await router.push('/dynamic/entry?from=run');
    await nextTick();
    expect(wrapper.text()).toBe('/dynamic/entry');

    wrapper.unmount();
  });
});
