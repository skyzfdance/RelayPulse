import type { Component, ComponentOptions, ComponentPublicInstance } from 'vue';

import { flushPromises } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, getCurrentInstance, h } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';

import { resetFeedbackForTests } from '@/composables/useFeedback';
import { setupRouterGuard } from '@/router/guard';
import { operationPage, reportOperationPageRenderFailure } from '@/router/operation-page';

import { deferred } from '../support/deferred';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke, isTauri: () => true }));
const disposals: (() => void)[] = [];
const reports = () =>
  native.invoke.mock.calls.filter(([command]) => command === 'report_operation_read_failure');

beforeEach(() => {
  native.invoke.mockReset().mockResolvedValue(undefined);
  resetFeedbackForTests();
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  resetFeedbackForTests();
  vi.restoreAllMocks();
});

function testPage(options: ComponentOptions): Component {
  return options;
}

function render(page: Component, activePage: Component = page) {
  const host = document.createElement('div');
  document.body.append(host);
  const app = createApp({ render: () => h(page) });
  app.config.errorHandler = (cause, instance, info) =>
    reportOperationPageRenderFailure(activePage, instance, cause, info);
  app.mount(host);
  const dispose = () => {
    app.unmount();
    host.remove();
  };
  disposals.push(dispose);
  return dispose;
}

function makeRouter(load: () => Promise<{ default: Component }>) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/records', component: operationPage('probe_records', load) },
      { path: '/other', component: testPage({ render: () => null }) },
    ],
  });
  setupRouterGuard(router);
  return router;
}

describe('页面独有失败的明确归属', () => {
  it('绑定的叶子页面模块加载失败才上报，普通导航守卫异常不冒充页面读取失败', async () => {
    const router = makeRouter(async () => {
      throw new Error('chunk load failed');
    });
    await expect(router.push('/records')).rejects.toThrow('业务页面模块加载失败');
    expect(reports()).toHaveLength(1);
    expect(reports()[0]?.[1]).toEqual({ input: { target: 'probe_records', kind: 'page' } });
    native.invoke.mockClear();
    router.beforeEach(() => {
      throw new Error('guard failed');
    });
    await expect(router.push('/other')).rejects.toThrow('guard failed');
    expect(reports()).toHaveLength(0);
  });

  it('已被新导航取代的页面模块加载失败不再上报', async () => {
    const pending = deferred<{ default: Component }>();
    const router = makeRouter(() => pending.promise);
    const oldNavigation = router.push('/records').catch(() => undefined);
    await flushPromises();
    await router.push('/other');
    pending.reject(new Error('late chunk failure'));
    await oldNavigation;
    expect(reports()).toHaveLength(0);
  });

  it.each(['setup', 'render'])('当前业务页根组件 %s 失败上报一次', async (kind) => {
    const page = testPage({
      setup() {
        if (kind === 'setup') throw new Error('page setup failed');
      },
      render() {
        if (kind === 'render') throw new Error('page render failed');
        return null;
      },
    });
    await operationPage('probe_records', async () => ({ default: page }))();
    render(page);
    expect(reports()).toHaveLength(1);
  });

  it('布局/全局宿主/页面子组件失败不归入当前业务页', async () => {
    const child = testPage({
      setup() {
        throw new Error('child failed');
      },
      render: () => null,
    });
    const page = testPage({ render: () => h(child) });
    await operationPage('probe_records', async () => ({ default: page }))();
    render(page);
    render(child, page);
    expect(reports()).toHaveLength(0);
  });

  it('旧页实例、已卸载实例与事件处理错误不归入活动页面；同实例不重复记', async () => {
    let instance: ComponentPublicInstance | null = null;
    const page = testPage({
      setup() {
        instance = getCurrentInstance()!.proxy;
      },
      render: () => null,
    });
    const other = testPage({ render: () => null });
    await operationPage('probe_records', async () => ({ default: page }))();
    await operationPage('general_settings', async () => ({ default: other }))();
    const stop = render(page);
    reportOperationPageRenderFailure(other, instance, new Error('old setup'), 'setup function');
    reportOperationPageRenderFailure(page, instance, new Error('click'), 'native event handler');
    expect(reports()).toHaveLength(0);
    reportOperationPageRenderFailure(
      page,
      instance,
      new Error('render'),
      'https://vuejs.org/error-reference/#runtime-1',
    );
    reportOperationPageRenderFailure(page, instance, new Error('render again'), 'render function');
    expect(reports()).toHaveLength(1);
    stop();
    await flushPromises();
    native.invoke.mockClear();
    reportOperationPageRenderFailure(page, instance, new Error('late setup'), 'setup function');
    expect(reports()).toHaveLength(0);
    disposals.pop();
  });
});
