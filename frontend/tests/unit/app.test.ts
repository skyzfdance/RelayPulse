import { flushPromises, mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { describe, expect, it } from 'vitest';
import { nextTick } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';

import { routes } from '@/router';
import { useThemeStore } from '@/stores/theme';

import App from '@/App.vue';

async function mountApp() {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes,
  });

  const wrapper = mount(App, {
    global: {
      plugins: [pinia, router],
    },
  });

  await router.isReady();
  return { wrapper, router, pinia };
}

describe('App 路由与布局骨架', () => {
  it('默认重定向到站点管理，头部读取 meta 标题，菜单按主路由分组生成', async () => {
    const { wrapper, router } = await mountApp();

    expect(router.currentRoute.value.path).toBe('/config/sites');
    // 头部标题与子标题由路由 meta 驱动
    expect(wrapper.find('h1').text()).toBe('站点管理');
    expect(wrapper.text()).toContain('配置多个模型站点，并通过拖动顺序决定故障时的优先级');
    // 只展示配置与运行分组，未完成模块由 meta.isMenu 隐藏
    const menu = wrapper.get('.ant-menu');
    expect(menu.text()).toContain('配置');
    expect(menu.text()).toContain('运行');
    expect(menu.text()).toContain('站点管理');
    expect(menu.text()).toContain('通用设置');
    expect(menu.text()).toContain('消息通知');
    expect(menu.text()).toContain('探测记录');
    expect(menu.text()).not.toContain('操作记录');
    expect(menu.text()).not.toContain('情报');
    expect(menu.text()).not.toContain('智力天梯');
    expect(menu.text()).not.toContain('测试');
    expect(menu.text()).not.toContain('历史记录');
    expect(menu.text()).not.toContain('题库管理');
    expect(wrapper.text()).not.toContain('页面不存在');
  });

  it('懒加载业务路由可解析，头部标题随路由切换', async () => {
    const { wrapper, router } = await mountApp();

    // 菜单点击 → 路由联动的真实交互由 e2e 覆盖，这里验证懒加载路由本身
    await router.push('/run/records');
    await flushPromises();
    expect(wrapper.find('h1').text()).toBe('探测记录');

    await router.push('/iq-test/bank');
    await flushPromises();
    expect(wrapper.find('h1').text()).toBe('题库管理');
  });

  it('隐藏路由测试详情可直达且不进菜单', async () => {
    const { wrapper, router } = await mountApp();

    await router.push('/iq-test/detail/abc');
    await flushPromises();

    expect(wrapper.find('h1').text()).toBe('测试详情');
    expect(wrapper.find('.ant-menu').text()).not.toContain('测试详情');
  });

  it('未知路由落入 404 独立整页（根记录无布局，错误页不带侧边栏）', async () => {
    const { wrapper, router } = await mountApp();

    await router.push('/definitely-not-exist');
    await router.isReady();

    expect(wrapper.text()).toContain('404');
    expect(wrapper.text()).toContain('抱歉，您访问的页面不存在。');
    expect(wrapper.text()).toContain('返回首页');
  });

  it('主题状态循环切换并同步 <html data-theme> 与持久化', async () => {
    localStorage.removeItem('relaypulse-vue-theme-mode');
    document.documentElement.removeAttribute('data-theme');

    const { pinia } = await mountApp();
    const themeStore = useThemeStore(pinia);

    // system + matchMedia（false）→ light
    expect(themeStore.mode).toBe('system');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    themeStore.cycleMode();
    await nextTick();
    expect(themeStore.mode).toBe('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    themeStore.cycleMode();
    await nextTick();
    expect(themeStore.mode).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(localStorage.getItem('relaypulse-vue-theme-mode')).toBe('dark');
  });
});
