import type { VueWrapper } from '@vue/test-utils';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, {
  DateRangePicker,
  Pagination,
  Segmented,
  Select,
  Table,
  Tooltip,
} from 'antdv-next';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter, RouterView } from 'vue-router';

import * as operationApi from '@/api/run/operation-logs';
import * as recordApi from '@/api/run/records';
import { seedProbeRecords } from '@/mocks/handlers/run/records';

import { deferred } from '../support/deferred';

import AppIcon from '@/components/AppIcon.vue';
import OperationHistoryView from '@/views/run/operation-history/OperationHistoryView.vue';
import RecordsView from '@/views/run/records/RecordsView.vue';

const mounted: (() => void)[] = [];
afterEach(() => {
  mounted
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  vi.restoreAllMocks();
  window.history.replaceState({}, '', '/');
});

async function mountRoutes(start: string) {
  seedProbeRecords();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      {
        path: '/run/operation-history',
        name: 'operation-history',
        component: OperationHistoryView,
      },
      { path: '/run/records', name: 'records', component: RecordsView },
    ],
  });
  await router.push(start);
  await router.isReady();
  const wrapper = mount(RouterView, {
    attachTo: document.body,
    global: { plugins: [router, AntdvNext] },
  });
  mounted.push(() => wrapper.unmount());
  if (start.startsWith('/run/operation-history')) {
    await vi.waitFor(() =>
      expect(
        wrapper.find('tbody tr[data-row-key]').exists() || wrapper.find('[role="alert"]').exists(),
      ).toBe(true),
    );
    if (wrapper.find('[role="alert"]').exists())
      throw new Error(`Operation history failed to load: ${wrapper.get('[role="alert"]').text()}`);
  } else await flushPromises();
  return { router, wrapper };
}

describe('operation history navigation', () => {
  it('opens the exact associated record set and returns to the same filtered log route', async () => {
    const lookup = vi.spyOn(recordApi, 'fetchProbeRecordsByIds');
    const { router, wrapper } = await mountRoutes(
      '/run/operation-history?source=system&actions=probe&page=1&pageSize=10',
    );
    const firstAction = wrapper.get('tbody tr[data-row-key="op-001"] button');
    const viewport = wrapper.get('[aria-label="操作记录列表"]').element;
    viewport.scrollTop = 187;
    await firstAction.trigger('click');
    await flushPromises();

    expect(lookup).toHaveBeenCalledWith(['probe-002', 'probe-003', 'probe-004']);
    expect(router.currentRoute.value.name).toBe('records');
    expect(wrapper.text()).toContain('已按事件关联 ID 定位 3 条探测记录');
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(3);

    const backButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('返回操作记录'))!;
    await backButton.trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.name).toBe('operation-history');
    expect(router.currentRoute.value.query).toMatchObject({
      source: 'system',
      actions: 'probe',
      page: '1',
      pageSize: '10',
    });
    expect(wrapper.get('[aria-label="操作记录列表"]').element.scrollTop).toBe(187);
  });

  it('opens the drawer for configuration events and displays the event snapshot', async () => {
    const { wrapper } = await mountRoutes('/run/operation-history');
    await wrapper.get('tbody tr[data-row-key="op-002"] button').trigger('click');
    await flushPromises();
    expect(document.body.textContent).toContain('修改站点连接配置');
    expect(document.body.textContent).not.toContain('修改前');
    expect(document.body.textContent).not.toContain('修改后');
    expect(document.querySelector('[role="group"][aria-label="修改前"]')).not.toBeNull();
    expect(
      [...document.querySelectorAll('[role="group"][aria-label="修改后"]')].some((group) =>
        group.textContent?.includes('已更换'),
      ),
    ).toBe(true);
  });

  it('renders backend field paths as configuration labels in the event drawer', async () => {
    const detail = await operationApi.fetchOperationLogDetail('op-002');
    vi.spyOn(operationApi, 'fetchOperationLogDetail').mockResolvedValueOnce({
      ...detail,
      content: {
        kind: 'changes',
        changes: [
          { field: 'models[1].name', before: '未配置', after: 'gpt-6.1-sol' },
          { field: 'models[1].enabled', before: '未配置', after: 'true' },
          { field: 'models[1].customRequest', before: '未配置', after: 'null' },
          { field: '凭据', before: '未配置', after: '已配置', sensitive: true },
        ],
      },
    });
    const { wrapper } = await mountRoutes('/run/operation-history');
    await wrapper.get('tbody tr[data-row-key="op-002"] button').trigger('click');
    await flushPromises();

    const event = document.querySelector('[aria-label="本次事件内容"]')!;
    expect(event.textContent).toContain('模型 2「gpt-6.1-sol」 · 自定义探针请求');
    expect(event.textContent).toContain('已启用');
    expect(event.textContent).toContain('未启用');
    expect(event.textContent).toContain('访问凭据');
    expect(event.textContent).not.toContain('models[1]');
    expect(event.textContent).not.toContain('null');
    expect(event.textContent).not.toContain('已更换');
  });

  it('opens the single linked record detail directly', async () => {
    const { wrapper } = await mountRoutes('/run/operation-history');
    await wrapper.get('tbody tr[data-row-key="op-009"] button').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('本次探测关联记录');
    expect(wrapper.text()).toContain('GPT-6 Astra');
    expect(document.body.textContent).toContain('探测详情');
  });

  it('resets page on filter and page-size changes and renders only the current page', async () => {
    const { router, wrapper } = await mountRoutes('/run/operation-history?page=2&pageSize=10');
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    wrapper.getComponent(Pagination).vm.$emit('change', 2, 20);
    await flushPromises();
    expect(router.currentRoute.value.query.page).toBe('1');
    expect(router.currentRoute.value.query.pageSize).toBe('20');
    const source = wrapper
      .findAllComponents(Select)
      .find((component) => component.vm.$attrs['aria-label'] === '操作来源')!;
    source.vm.$emit('change', ['system', 'user']);
    await flushPromises();
    expect(router.currentRoute.value.query.source).toBe('system,user');
    expect(router.currentRoute.value.query.page).toBe('1');
    expect(
      wrapper
        .findAllComponents(Select)
        .filter((component) => {
          const label = component.vm.$attrs['aria-label'];
          return typeof label === 'string' && ['操作来源', '执行结果', '操作动作'].includes(label);
        })
        .every((component) => component.props('mode') === 'multiple'),
    ).toBe(true);
    expect(wrapper.getComponent(Pagination).props('showQuickJumper')).toBe(false);
    expect(wrapper.getComponent(Pagination).props('size')).toBe('small');
  });

  it('renders compact filter prompts with one date range and recent-range presets', async () => {
    const { wrapper } = await mountRoutes('/run/operation-history');

    expect(wrapper.getComponent(DateRangePicker).props('placeholder')).toEqual([
      '开始日期',
      '结束日期',
    ]);
    for (const [label, placeholder] of [
      ['操作来源', '全部来源'],
      ['执行结果', '全部结果'],
      ['操作动作', '全部动作'],
    ]) {
      const select = wrapper
        .findAllComponents(Select)
        .find((component) => component.vm.$attrs['aria-label'] === label);
      expect(select?.props('placeholder')).toBe(placeholder);
    }
    expect(wrapper.text()).not.toContain('关键词');
    expect(wrapper.text()).not.toContain('重置筛选');
    expect(wrapper.findAll('.ant-picker-range')).toHaveLength(1);
    expect(wrapper.getComponent(DateRangePicker).props('presets')).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ label: '最近一周' }),
        expect.objectContaining({ label: '最近15天' }),
        expect.objectContaining({ label: '最近30天' }),
      ]),
    );
  });

  it('serializes the date range and action multiselect while returning to page one', async () => {
    const { router, wrapper } = await mountRoutes('/run/operation-history?page=2&pageSize=10');

    wrapper.getComponent(DateRangePicker).vm.$emit('change', ['2026-11-01', '2026-11-04']);
    await flushPromises();
    expect(router.currentRoute.value.query).toMatchObject({
      startDate: '2026-11-01',
      endDate: '2026-11-04',
      page: '1',
    });

    const action = wrapper
      .findAllComponents(Select)
      .find((component) => component.vm.$attrs['aria-label'] === '操作动作')!;
    action.vm.$emit('change', ['probe', 'update']);
    await flushPromises();
    expect(router.currentRoute.value.query.actions).toBe('probe,update');
    expect(router.currentRoute.value.query.page).toBe('1');
  });

  it('shows summary as the main column and keeps one detail icon fixed at the end', async () => {
    const { wrapper } = await mountRoutes('/run/operation-history');
    expect(wrapper.findAll('thead th').map((header) => header.text())).toEqual([
      '来源',
      '动作',
      '操作概要',
      '结果',
      '时间',
      '操作',
    ]);
    const probe = wrapper.get('tbody tr[data-row-key="op-001"]');
    const changes = wrapper.get('tbody tr[data-row-key="op-002"]');
    expect(probe.findAll('td')[1].text()).toBe('探测');
    expect(probe.findAll('td')[2].text()).toBe('系统完成站点 doudui.cc 的自动探测。');
    expect(changes.findAll('td')[1].text()).toBe('修改');
    expect(changes.findAll('td')[2].text()).toContain('ai.input.im');
    expect(probe.get('button').attributes('aria-label')).toBe('查看关联探测记录');
    expect(changes.get('button').attributes('aria-label')).toBe('查看操作详情');
    expect(probe.get('button').text()).toBe('');
    expect(changes.get('button').text()).toBe('');
    expect(probe.getComponent(AppIcon).props('name')).toBe('FileSearchOutlined');
    expect(changes.getComponent(AppIcon).props('name')).toBe('FileSearchOutlined');
    const tooltips: VueWrapper[] = probe.findAllComponents(Tooltip);
    expect(tooltips.map((tooltip) => (tooltip.props() as { title?: string }).title)).toEqual([
      '查看关联探测记录',
    ]);
    expect(probe.findAll('td')[2].get('.rp-overflow-tooltip').attributes('title')).toBeUndefined();
    const table = wrapper.getComponent(Table);
    const columns = table.props('columns')!;
    expect(columns[columns.length - 1]).toMatchObject({ key: 'operation', fixed: 'end' });
    expect(columns.find((column) => column.key === 'summary')?.width).toBeGreaterThan(
      Number(columns.find((column) => column.key === 'occurredAt')?.width),
    );
    expect(table.props('scroll')?.x).toBeGreaterThan(0);
  });

  it('leaves association mode and restores ordinary record queries', async () => {
    const { wrapper } = await mountRoutes('/run/records?recordIds=probe-008');
    await vi.waitFor(() => expect(wrapper.text()).toContain('本次探测关联记录'));
    const showAll = wrapper
      .findAll('button')
      .find((button) => button.text().includes('查看全部探测记录'))!;
    await showAll.trigger('click');
    await vi.waitFor(() => expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(20));
    expect(wrapper.find('[aria-label="模型筛选"]').exists()).toBe(true);
  });

  it('reports missing associations rather than displaying the default list', async () => {
    const { wrapper } = await mountRoutes('/run/records?recordIds=probe-expired-001');
    await vi.waitFor(() => expect(wrapper.text()).toContain('1 条关联记录已过期或清理'));
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(0);
  });

  it('isolates overlapping association lookups when a newer selection arrives', async () => {
    const oldLookup = deferred<Awaited<ReturnType<typeof recordApi.fetchProbeRecordsByIds>>>();
    const newLookup = deferred<Awaited<ReturnType<typeof recordApi.fetchProbeRecordsByIds>>>();
    const lookup = vi.spyOn(recordApi, 'fetchProbeRecordsByIds');
    lookup.mockReturnValueOnce(oldLookup.promise).mockReturnValueOnce(newLookup.promise);
    const { router, wrapper } = await mountRoutes(
      '/run/records?recordIds=old-id&fromOperationHistory=1',
    );
    await router.push({
      path: '/run/records',
      query: { recordIds: 'new-id,new-second-id', fromOperationHistory: '1' },
    });
    const item = {
      id: 'new-id',
      siteName: 'New linked site',
      modelName: 'model',
      probedAt: new Date().toISOString(),
      result: 'success' as const,
      resultMessage: '请求成功',
      firstTokenMs: 100,
      durationMs: 200,
      totalTokens: null,
    };
    newLookup.resolve({ items: [item], missingIds: [] });
    await flushPromises();
    oldLookup.resolve({
      items: [{ ...item, id: 'old-id', siteName: 'Old linked site' }],
      missingIds: [],
    });
    await flushPromises();
    expect(wrapper.find('tr[data-row-key="new-id"]').exists()).toBe(true);
    expect(wrapper.find('tr[data-row-key="old-id"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('New linked site');
    expect(wrapper.text()).not.toContain('Old linked site');
  });

  it('switches from Token statistics to the requested record when receiving new IDs', async () => {
    const { router, wrapper } = await mountRoutes('/run/records');
    wrapper.getComponent(Segmented).vm.$emit('update:value', 'tokens');
    await flushPromises();
    await router.push('/run/records?recordIds=probe-008');
    await vi.waitFor(() =>
      expect(wrapper.find('tr[data-row-key="probe-008"]').exists()).toBe(true),
    );
    expect(wrapper.text()).toContain('本次探测关联记录');
  });
});
