import type { IqParticipant } from '@/types/domain/iq-test';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Button, Drawer, InputNumber, Select } from 'antdv-next';
import { createPinia, disposePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter, RouterView } from 'vue-router';

import { resetEventBusForTests } from '@/api/events';
import * as taskApi from '@/api/iq-test/task';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state as sitesState } from '@/mocks/handlers/config/sites';
import { seedIqState } from '@/mocks/handlers/iq-test';
import { useIqTestStore } from '@/stores/iq-test';

import { deferred } from '../../support/deferred';
import { chatPolicy } from '../../support/parameter-fields';

import IqTestView from '@/views/iq-test/task/IqTestView.vue';

const cleanup: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/iq-test/task');
  resetEventBusForTests();
  resetFeedbackForTests();
  seedSites();
  seedIqState();
});
afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((fn) => fn());
  seedIqState();
  vi.restoreAllMocks();
  vi.useRealTimers();
  resetEventBusForTests();
  resetFeedbackForTests();
  document.body.innerHTML = '';
  window.history.replaceState({}, '', '/');
});
async function setup() {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/iq-test/task', component: IqTestView },
      { path: '/another-page', component: { template: '<div>其他页面</div>' } },
      { path: '/iq-test/bank', component: { template: '<div>题库</div>' } },
    ],
  });
  await router.push('/iq-test/task');
  await router.isReady();
  const wrapper = mount(RouterView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext, pinia, router] },
  });
  cleanup.push(
    () => disposePinia(pinia),
    () => wrapper.unmount(),
  );
  await flushPromises();
  const store = useIqTestStore(pinia);
  await store.connect();
  await store.refresh();
  await flushPromises();
  return { wrapper, router, store };
}
type TestPage = Awaited<ReturnType<typeof setup>>;
function button(page: TestPage, label: string) {
  return page.wrapper
    .findAllComponents(Button)
    .find((item) => item.text().replace(/\s/gu, '') === label)!;
}
async function openDrawer(page: TestPage) {
  await button(page, '添加参与者').trigger('click');
  await flushPromises();
  return new DOMWrapper(document.querySelector('.ant-drawer')!);
}
async function choose(page: TestPage, label: string, value: string) {
  const select = page.wrapper
    .findAllComponents(Select)
    .find((item) => item.vm.$attrs['aria-label'] === label)!;
  select.vm.$emit('update:value', value);
  await flushPromises();
}

describe('IQ 配置页与参与者抽屉', () => {
  it('添加后预检等待期间显示旋转图标，返回结果后恢复可用状态', async () => {
    const pending = deferred<IqParticipant>();
    vi.spyOn(taskApi, 'runIqPrecheck').mockReturnValueOnce(pending.promise);
    const page = await setup();
    const drawer = await openDrawer(page);
    await choose(page, '参与者模型', 'site-1/model-0-0');
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    await flushPromises();
    const participant = page.store.participants[0]!;
    const row = page.wrapper.get(`tr[data-row-key="${participant.id}"]`);
    expect(row.text()).toContain('检测中');
    expect(row.find('.animate-spin').exists()).toBe(true);
    pending.resolve({
      ...participant,
      precheck: { status: 'available', message: '', checkedAtMs: Date.now() },
    });
    await flushPromises();
    expect(row.text()).toContain('可用');
    expect(row.find('.animate-spin').exists()).toBe(false);
  });

  it('预览区域常驻，参数无效时提示修正，恢复有效后展示独立 IQ 参数', async () => {
    const site = sitesState.sites[0]!;
    site.policy = chatPolicy('站点探针内容', 8, { temperature: 0.9 });
    const originalPolicy = JSON.stringify(site.policy);
    const create = vi.spyOn(taskApi, 'createIqParticipant');
    const page = await setup();
    const drawer = await openDrawer(page);
    const preview = () => drawer.get('[aria-label="IQ 参数预览"]');
    const submit = () => drawer.get('.ant-drawer-footer .ant-btn-primary');
    const addParameter = () =>
      drawer
        .findAll('button')
        .find((item) => item.text().includes('添加参数'))!
        .trigger('click');

    expect(preview().text()).toBe('请先选择模型后查看预览。');
    await choose(page, '参与者模型', 'site-1/model-0-0');
    expect(JSON.parse(preview().text())).toEqual({ reasoning_effort: 'high' });

    await addParameter();
    expect(drawer.get('[role="alert"]').text()).toBe('请输入参数字段');
    expect(preview().text()).toBe('请先修正配置后查看预览。');
    expect(submit().attributes('disabled')).toBeDefined();

    const row = drawer.get('[data-parameter-key=""]');
    await row.get('[aria-label^="参数字段 "]').setValue('asu');
    await row.get('[aria-label^="参数值 "]').setValue('d');
    expect(JSON.parse(preview().text())).toEqual({ reasoning_effort: 'high', asu: 'd' });
    expect(submit().attributes('disabled')).toBeUndefined();

    await addParameter();
    expect(preview().text()).toBe('请先修正配置后查看预览。');
    expect(submit().attributes('disabled')).toBeDefined();
    await drawer.get('[data-parameter-key=""] button[title="删除参数"]').trigger('click');
    expect(JSON.parse(preview().text())).toEqual({ reasoning_effort: 'high', asu: 'd' });
    expect(submit().attributes('disabled')).toBeUndefined();
    expect(JSON.stringify(site.policy)).toBe(originalPolicy);
    expect(create).not.toHaveBeenCalled();
  });

  it('独立配置参数、失败重探测后开始；切页返回恢复相同运行快照', async () => {
    window.history.replaceState({}, '', '/iq-test/task?iqScenario=precheck-failure');
    const site = sitesState.sites[0]!;
    site.enabled = false;
    site.models[0]!.health = 'error';
    site.policy = chatPolicy('探针题面不能带入IQ', 8, {
      temperature: 0.9,
      reasoning_effort: 'minimal',
    });
    const originalPolicy = JSON.stringify(site.policy);
    const create = vi.spyOn(taskApi, 'createIqParticipant');
    const precheck = vi.spyOn(taskApi, 'runIqPrecheck');
    const start = vi.spyOn(taskApi, 'startIqRun');
    const page = await setup();
    expect(button(page, '开始测试').attributes('disabled')).toBeDefined();
    expect(
      page.wrapper.get('tr[data-row-key="iq-question-4"] input[type="checkbox"]'),
    ).toBeDefined();
    expect(
      page.wrapper
        .get('tr[data-row-key="iq-question-4"] input[type="checkbox"]')
        .attributes('disabled'),
    ).toBeDefined();
    await page.wrapper
      .get('tr[data-row-key="iq-question-1"] input[type="checkbox"]')
      .setValue(true);
    const drawer = await openDrawer(page);
    await choose(page, '参与者模型', 'site-1/model-0-0');
    expect(drawer.text()).toContain('暂无手写参数');
    expect(drawer.get('[aria-label="IQ 参数预览"]').text()).toBe(
      '{\n  "reasoning_effort": "high"\n}',
    );
    expect(drawer.text()).not.toContain('探针题面不能带入IQ');
    await drawer
      .findAll('button')
      .find((item) => item.text().includes('添加参数'))!
      .trigger('click');
    await drawer.get('input[aria-label="参数字段 1"]').setValue('reasoning_effort');
    await drawer.get('[aria-label="参数值 1"]').setValue('low');
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    await flushPromises();
    expect(create).toHaveBeenCalledExactlyOnceWith({
      modelKey: 'site-1/model-0-0',
      preset: '高 (high)',
      parameters: { reasoning_effort: 'low' },
    });
    expect(page.store.participants).toHaveLength(1);
    expect(page.store.participants[0]!.precheck.status).toBe('unavailable');
    expect(page.wrapper.text()).toContain('不可用');
    expect(button(page, '开始测试').attributes('disabled')).toBeDefined();
    expect(JSON.stringify(site.policy)).toBe(originalPolicy);
    const oldSnapshot = page.store.participants[0]!.snapshotId;
    site.name = '修正后的源站点';
    await page.wrapper.get('[aria-label="重新探测"]').trigger('click');
    await flushPromises();
    const current = page.store.participants[0]!;
    expect(current.snapshotId).not.toBe(oldSnapshot);
    expect(current.siteName).toBe('修正后的源站点');
    expect(current.precheck.status).toBe('available');
    expect(precheck).toHaveBeenCalledTimes(2);
    expect(button(page, '开始测试').attributes('disabled')).toBeUndefined();
    expect(page.wrapper.findComponent(InputNumber).props('value')).toBe(5);
    expect(page.wrapper.find('[aria-label="超时秒数"]').exists()).toBe(false);
    page.wrapper.findComponent(InputNumber).vm.$emit('update:value', 2);
    await flushPromises();
    // 窄化冻结 mock 推进与展示计时，不冻结 Antdv 的交互和动画定时器。
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    await button(page, '开始测试').trigger('click');
    await flushPromises();
    expect(start).toHaveBeenCalledExactlyOnceWith({
      questionIds: ['iq-question-1'],
      retryMax: 2,
    });
    const run = page.store.activeRun!;
    expect(run.participants[0]!.snapshotId).toBe(current.snapshotId);
    expect(page.wrapper.get('[aria-label="运行进度"]').text()).toContain('测试进行中');
    await page.router.push('/another-page');
    await flushPromises();
    site.models = [];
    await page.router.push('/iq-test/task');
    await flushPromises();
    expect(page.wrapper.find('[aria-label="运行进度"]').exists()).toBe(true);
    expect(page.store.activeRun?.id).toBe(run.id);
    expect(page.store.activeRun?.participants[0]?.snapshotId).toBe(current.snapshotId);
    expect(precheck).toHaveBeenCalledTimes(2);
    expect(button(page, '开始测试')).toBeUndefined();
  }, 15000);

  it('参数表禁止覆盖系统保留路径，禁用该字段后才可添加', async () => {
    const page = await setup();
    const create = vi.spyOn(taskApi, 'createIqParticipant');
    const drawer = await openDrawer(page);
    await choose(page, '参与者模型', 'site-1/model-0-0');
    await drawer
      .findAll('button')
      .find((item) => item.text().includes('添加参数'))!
      .trigger('click');
    await drawer.get('input[aria-label="参数字段 1"]').setValue('messages');
    await drawer.get('[aria-label="参数值 1"]').setValue('替换题面');
    expect(drawer.get('[role="alert"]').text()).toContain('系统字段不可覆盖');
    expect(drawer.get('.ant-drawer-footer .ant-btn-primary').attributes('disabled')).toBeDefined();
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    expect(create).not.toHaveBeenCalled();
    await drawer.get('[aria-label="启用参数 messages"] input').setValue(false);
    expect(drawer.find('[role="alert"]').exists()).toBe(false);
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    await flushPromises();
    expect(create).toHaveBeenCalledExactlyOnceWith({
      modelKey: 'site-1/model-0-0',
      preset: '高 (high)',
      parameters: { reasoning_effort: 'high' },
    });
  });

  it('后台快照刷新不重置正在编辑的推理档位与手写参数', async () => {
    const page = await setup();
    const drawer = await openDrawer(page);
    await choose(page, '参与者模型', 'site-1/model-0-0');
    await choose(page, '推理强度', 'max');
    expect(drawer.get('[aria-label="IQ 参数预览"]').text()).toContain('max');
    await page.store.refresh();
    await flushPromises();
    expect(drawer.get('[aria-label="IQ 参数预览"]').text()).toContain('max');
    const select = page.wrapper
      .findAllComponents(Select)
      .find((item) => item.vm.$attrs['aria-label'] === '推理强度')!;
    expect(select.props('value')).toBe('max');
  });

  it('添加提交等待期间不能通过关闭按钮或ESC丢弃草稿', async () => {
    const page = await setup();
    const drawer = await openDrawer(page);
    await choose(page, '参与者模型', 'site-1/model-0-0');
    const pending = deferred<IqParticipant>();
    vi.spyOn(taskApi, 'createIqParticipant').mockReturnValueOnce(pending.promise);
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    await flushPromises();
    const component = page.wrapper.getComponent(Drawer);
    expect(component.props('closable')).toMatchObject({ disabled: true });
    expect(component.props('keyboard')).toBe(false);
    pending.reject(new Error('模拟创建失败'));
    await flushPromises();
    expect(component.props('open')).toBe(true);
    expect(
      drawer.get('.ant-drawer-footer .ant-btn-primary').attributes('disabled'),
    ).toBeUndefined();
  });
});
