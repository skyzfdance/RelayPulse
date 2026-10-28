import type { MainWindowRoute } from '@/api/main-window';

import { flushPromises, mount } from '@vue/test-utils';
import { Segmented, Select } from 'antdv-next';
import { createPinia } from 'pinia';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter } from 'vue-router';

import * as sitesApi from '@/api/config/sites';
import * as recordsApi from '@/api/run/records';
import { resetGlobalLoadingForTests, useGlobalLoading } from '@/composables/useGlobalLoading';
import { seedSites } from '@/mocks/handlers/config/sites';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { routes } from '@/router';

import { deferred } from '../support/deferred';

import App from '@/App.vue';
import SitesView from '@/views/config/sites/SitesView.vue';
import ProbeRecordsTable from '@/views/run/records/components/ProbeRecordsTable.vue';
import RecordsView from '@/views/run/records/RecordsView.vue';

const navigation = vi.hoisted(() => ({
  handler: undefined as ((route: MainWindowRoute) => void) | undefined,
  stop: vi.fn(),
}));
vi.mock('@/api/main-window', () => ({
  subscribeMainWindowNavigation: (handler: (route: MainWindowRoute) => void) => {
    navigation.handler = handler;
    return Promise.resolve(navigation.stop);
  },
}));
const disposals: (() => void)[] = [];
beforeEach(() => {
  seedSites();
  seedProbeRecords();
  resetGlobalLoadingForTests();
  navigation.stop.mockClear();
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  resetGlobalLoadingForTests();
});
async function open(route: string) {
  const router = createRouter({ history: createMemoryHistory(), routes });
  await router.push(route);
  const wrapper = mount(App, { global: { plugins: [createPinia(), router] } });
  disposals.push(() => wrapper.unmount());
  await flushPromises();
  return { router, wrapper };
}

it('通知点击经 Vue 路由进入记录页，应用根节点保留，卸载后释放监听', async () => {
  const { router, wrapper } = await open('/config/sites');
  const root = wrapper.vm.$.uid;
  navigation.handler?.('/run/records');
  await flushPromises();
  expect(router.currentRoute.value.path).toBe('/run/records');
  expect(wrapper.findComponent(RecordsView).exists()).toBe(true);
  expect(wrapper.vm.$.uid).toBe(root);
  expect(useGlobalLoading().visible.value).toBe(false);
  disposals.pop()?.();
  expect(navigation.stop).toHaveBeenCalledOnce();
});

it('已在记录页时保留实例、筛选及现有数据，只切回记录 Tab 并重新取数', async () => {
  const { wrapper } = await open('/run/records');
  const view = wrapper.getComponent(RecordsView);
  const model = view
    .findAllComponents(Select)
    .find((item) => item.vm.$attrs['aria-label'] === '模型筛选')!;
  model.vm.$emit('change', 'GPT-6 Astra');
  await flushPromises();
  const previous = view.getComponent(ProbeRecordsTable).props('data');
  view.getComponent(Segmented).vm.$emit('update:value', 'tokens');
  await flushPromises();
  const pending = deferred<Awaited<ReturnType<typeof recordsApi.fetchProbeRecords>>>();
  const read = vi.spyOn(recordsApi, 'fetchProbeRecords').mockReturnValueOnce(pending.promise);
  navigation.handler?.('/run/records');
  await flushPromises();
  expect(wrapper.getComponent(RecordsView).vm.$.uid).toBe(view.vm.$.uid);
  expect(view.getComponent(ProbeRecordsTable).props('data')).toBe(previous);
  expect(read).toHaveBeenCalledExactlyOnceWith(
    expect.objectContaining({ modelName: 'GPT-6 Astra' }),
  );
  expect(useGlobalLoading().visible.value).toBe(false);
  pending.resolve(previous!);
  await flushPromises();
});

it('查看全部在站点页静默刷新列表和详情，保留选中站点、表单草稿且不显示骨架', async () => {
  const { wrapper } = await open('/config/sites');
  await vi.waitFor(() => expect(wrapper.find('input#name').exists()).toBe(true));
  const view = wrapper.getComponent(SitesView);
  await wrapper.get('input#name').setValue('尚未保存的名称');
  const current = await sitesApi.fetchSites();
  const detail = await sitesApi.fetchSite(current[0]!.id);
  const pending = deferred<Awaited<ReturnType<typeof sitesApi.fetchSite>>>();
  const read = vi.spyOn(sitesApi, 'fetchSite').mockReturnValueOnce(pending.promise);
  navigation.handler?.('/config/sites');
  await flushPromises();
  expect(wrapper.getComponent(SitesView).vm.$.uid).toBe(view.vm.$.uid);
  expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('尚未保存的名称');
  expect(wrapper.find('[aria-label="正在加载站点"]').exists()).toBe(false);
  expect(wrapper.find('[aria-label="正在加载站点详情"]').exists()).toBe(false);
  expect(read).toHaveBeenCalledWith(current[0]!.id);
  pending.resolve(detail);
  await flushPromises();
  expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('尚未保存的名称');
});

it('原有离开保护可以阻止通知导航，不通过原生重载绕过保护', async () => {
  const { router, wrapper } = await open('/config/general');
  router.beforeEach(() => false);
  navigation.handler?.('/run/records');
  await flushPromises();
  expect(router.currentRoute.value.path).toBe('/config/general');
  expect(wrapper.findComponent(RecordsView).exists()).toBe(false);
});
