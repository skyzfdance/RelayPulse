import type { MainWindowNavigationTarget } from '@/api/main-window';

import { flushPromises, mount } from '@vue/test-utils';
import { Segmented, Select } from 'antdv-next';
import { createPinia } from 'pinia';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter } from 'vue-router';

import * as sitesApi from '@/api/config/sites';
import * as iqHistoryApi from '@/api/iq-test/history';
import * as iqTaskApi from '@/api/iq-test/task';
import * as recordsApi from '@/api/run/records';
import { resetGlobalLoadingForTests, useGlobalLoading } from '@/composables/useGlobalLoading';
import { seedSites } from '@/mocks/handlers/config/sites';
import { advanceIqRun, seedIqState } from '@/mocks/handlers/iq-test';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { routes } from '@/router';

import { deferred } from '../support/deferred';

import App from '@/App.vue';
import SitesView from '@/views/config/sites/SitesView.vue';
import IqHistoryView from '@/views/iq-test/history/IqHistoryView.vue';
import ProbeRecordsTable from '@/views/run/records/components/ProbeRecordsTable.vue';
import RecordsView from '@/views/run/records/RecordsView.vue';

const navigation = vi.hoisted(() => ({
  handler: undefined as ((target: MainWindowNavigationTarget) => void) | undefined,
  stop: vi.fn(),
}));
vi.mock('@/api/main-window', () => ({
  subscribeMainWindowNavigation: (handler: (target: MainWindowNavigationTarget) => void) => {
    navigation.handler = handler;
    return Promise.resolve(navigation.stop);
  },
}));
const disposals: (() => void)[] = [];
beforeEach(() => {
  seedSites();
  seedIqState();
  seedProbeRecords();
  resetGlobalLoadingForTests();
  navigation.stop.mockClear();
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  seedIqState();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  resetGlobalLoadingForTests();
});
async function open(route: string) {
  const router = createRouter({ history: createMemoryHistory(), routes });
  await router.push(route);
  const wrapper = mount(App, { global: { plugins: [createPinia(), router] } });
  disposals.push(() => wrapper.unmount());
  await vi.waitFor(() => expect(wrapper.find('[aria-label="应用启动"]').exists()).toBe(false));
  await flushPromises();
  return { router, wrapper };
}

async function completedIqRun() {
  const participant = await iqTaskApi.createIqParticipant({
    modelKey: 'site-1/model-0-0',
    headers: [],
    preset: 'high',
    parameters: { reasoning_effort: 'high' },
  });
  await iqTaskApi.runIqPrecheck(participant.snapshotId);
  const run = await iqTaskApi.startIqRun({
    questionIds: ['iq-question-1'],
    retryMax: 0,
  });
  for (let tick = 0; tick < run.participants.length * 2 + 1; tick++) advanceIqRun();
  return run;
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

it('系统通知携带运行标识进入对应 IQ 详情，不创建应用内完成浮层', async () => {
  const run = await completedIqRun();
  const { router, wrapper } = await open('/config/sites');
  const root = wrapper.vm.$.uid;
  const read = vi.spyOn(iqHistoryApi, 'fetchIqRun');
  navigation.handler?.({ path: '/iq-test/history', query: { runId: run.id } });
  await flushPromises();
  expect(router.currentRoute.value.path).toBe('/iq-test/history');
  expect(router.currentRoute.value.query).toEqual({ runId: run.id });
  expect(read).toHaveBeenCalledWith(run.id);
  expect(wrapper.findComponent(IqHistoryView).exists()).toBe(true);
  expect(document.querySelector('.ant-drawer-body')?.textContent).toContain(
    run.questions[0]!.content,
  );
  expect(wrapper.vm.$.uid).toBe(root);
  expect(document.querySelector('[aria-label="智商测试完成提醒"]')).toBeNull();
});

it('已在 IQ 历史时通知切换到另一场详情，保留页面与现有筛选', async () => {
  const previous = await completedIqRun();
  const next = await completedIqRun();
  const read = vi.spyOn(iqHistoryApi, 'fetchIqRun');
  const { router, wrapper } = await open(`/iq-test/history?runId=${previous.id}`);
  const view = wrapper.getComponent(IqHistoryView);
  const result = view
    .findAllComponents(Select)
    .find((item) => item.vm.$attrs['aria-label'] === 'IQ 执行结果筛选')!;
  result.vm.$emit('update:value', 'all-success');
  await flushPromises();
  read.mockClear();
  navigation.handler?.({ path: '/iq-test/history', query: { runId: next.id } });
  await flushPromises();
  expect(router.currentRoute.value.query).toEqual({ runId: next.id });
  expect(read).toHaveBeenCalledWith(next.id);
  expect(wrapper.getComponent(IqHistoryView).vm.$.uid).toBe(view.vm.$.uid);
  expect(result.props('value')).toBe('all-success');
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
