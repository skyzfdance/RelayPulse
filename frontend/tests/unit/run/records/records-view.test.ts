import type { ProbeRecordListItem, RecordsPage } from '@/types/domain/records';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Drawer, Dropdown, Modal, Segmented, Select } from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as api from '@/api/run/records';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { recordsState, seedProbeRecords } from '@/mocks/handlers/run/records';
import { createProbeRecordFixtures } from '@/mocks/handlers/run/records-fixtures';

import { deferred } from '../../../support/deferred';

import ProbeRecordsTable from '@/views/run/records/components/ProbeRecordsTable.vue';
import TokenStatsTable from '@/views/run/records/components/TokenStatsTable.vue';
import RecordsView from '@/views/run/records/RecordsView.vue';

const disposals: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/run/records');
  seedProbeRecords();
  resetFeedbackForTests();
});
afterEach(() => {
  disposals
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  vi.restoreAllMocks();
  vi.useRealTimers();
  resetFeedbackForTests();
  window.history.replaceState({}, '', '/');
});
async function mountPage({ waitForRecords = true } = {}) {
  const wrapper = mount(RecordsView, {
    attachTo: document.body,
    global: {
      plugins: [AntdvNext],
    },
  });
  disposals.push(() => wrapper.unmount());
  await flushPromises();
  if (waitForRecords)
    await vi.waitFor(() =>
      expect(wrapper.get('[aria-label="刷新探测记录和 Token 统计"]').classes()).not.toContain(
        'ant-btn-loading',
      ),
    );
  return { wrapper };
}
type PageWrapper = Awaited<ReturnType<typeof mountPage>>['wrapper'];
function recordData(wrapper: PageWrapper) {
  return wrapper.getComponent(ProbeRecordsTable).props('data');
}
function tokenData(wrapper: PageWrapper) {
  return wrapper.getComponent(TokenStatsTable).props('data');
}
function modalButton(label: string) {
  const button = [...document.querySelectorAll<HTMLButtonElement>('.ant-modal-footer button')].find(
    (button) => button.textContent?.replace(/\s/gu, '') === label,
  );
  if (!button) throw new Error(`未找到弹窗按钮：${label}`);
  return new DOMWrapper(button);
}
async function choose(
  wrapper: Awaited<ReturnType<typeof mountPage>>['wrapper'],
  label: string,
  value: string,
) {
  const select = wrapper
    .findAllComponents(Select)
    .find((select) => select.vm.$attrs['aria-label'] === label)!;
  select.vm.$emit('change', value);
  await flushPromises();
}
async function switchTab(
  wrapper: Awaited<ReturnType<typeof mountPage>>['wrapper'],
  value: 'records' | 'tokens',
) {
  wrapper.getComponent(Segmented).vm.$emit('update:value', value);
  await flushPromises();
}

describe('探测记录页面交互', () => {
  it('默认按探测时间降序加载和分页，表头不显示排序且点击不会重新查询', async () => {
    const { wrapper } = await mountPage();
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    expect(wrapper.text()).not.toContain('记录保留');
    expect(wrapper.find('input[aria-label="搜索模型或错误"]').exists()).toBe(false);
    expect(wrapper.find('thead')!.text()).not.toContain('__v_isVNode');
    expect(wrapper.findAll('thead .ant-table-column-sorter')).toHaveLength(0);
    expect(wrapper.find('thead [aria-sort]').exists()).toBe(false);
    const read = vi.spyOn(api, 'fetchProbeRecords');
    const timeHeader = wrapper.findAll('th').find((cell) => cell.text() === '探测时间')!;
    await timeHeader.trigger('click');
    await flushPromises();
    expect(read).not.toHaveBeenCalled();
    expect(recordData(wrapper)?.items[0].id).toBe('probe-001');
    await wrapper.get('.ant-pagination-item-2').trigger('click');
    await flushPromises();
    expect(recordData(wrapper)?.page).toBe(2);
    expect(recordData(wrapper)?.items[0].id).toBe('probe-011');
    const tokenHeader = wrapper.findAll('th').find((cell) => cell.text() === 'Token')!;
    await tokenHeader.trigger('click');
    await flushPromises();
    expect(recordData(wrapper)?.page).toBe(2);
    expect(read).toHaveBeenCalledOnce();
    expect(recordData(wrapper)?.items[0].id).toBe('probe-011');
  });

  it('模型、结果仅筛选记录；Token 视图只保留共享时间与站点条件', async () => {
    const { wrapper } = await mountPage();
    await switchTab(wrapper, 'tokens');
    const stats = tokenData(wrapper)?.summary;
    await switchTab(wrapper, 'records');
    await choose(wrapper, '模型筛选', 'GPT-6 Astra');
    await choose(wrapper, '探测结果筛选', 'failure');
    expect(
      recordData(wrapper)?.items.every(
        (record) => record.modelName === 'GPT-6 Astra' && record.result === 'failure',
      ),
    ).toBe(true);
    await switchTab(wrapper, 'tokens');
    expect(wrapper.findAllComponents(Select)).toHaveLength(2);
    expect(wrapper.find('input[aria-label="搜索模型或错误"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('记录保留');
    expect(tokenData(wrapper)?.summary).toEqual(stats);
    expect(wrapper.findAll('thead .ant-table-column-sorter')).toHaveLength(0);
    expect(wrapper.find('thead [aria-sort]').exists()).toBe(false);
    const read = vi.spyOn(api, 'fetchProbeTokenStats');
    const totalHeader = wrapper.findAll('th').find((cell) => cell.text() === '总 Token')!;
    await totalHeader.trigger('click');
    await flushPromises();
    expect(read).not.toHaveBeenCalled();
    expect(tokenData(wrapper)?.items[0].siteId).toBe('site-1');
    await choose(wrapper, '站点筛选', 'site-3');
    expect(tokenData(wrapper)?.items).toHaveLength(1);
    expect(tokenData(wrapper)?.items[0].siteId).toBe('site-3');
    await switchTab(wrapper, 'records');
    const model = wrapper
      .findAllComponents(Select)
      .find((select) => select.vm.$attrs['aria-label'] === '模型筛选')!;
    expect(model.props('value')).toBe('');
    expect(model.props('options')?.map((option) => option.value)).toEqual([
      '',
      'Claude Opus',
      'Claude Sonnet',
    ]);
  });

  it('Token 合计覆盖全部分页，随筛选更新，平均值按已上报总量的请求计算', async () => {
    const first = createProbeRecordFixtures()[0];
    const records = Array.from({ length: 14 }, (_, index) => ({
      ...first,
      id: `summary-${index}`,
      siteId: `summary-site-${index}`,
      siteName: `统计站点 ${index}`,
      usage: { inputTokens: index * 10, outputTokens: index, totalTokens: index * 11 },
    }));
    seedProbeRecords([
      ...records,
      {
        ...records[0],
        id: 'summary-extra',
        usage: { inputTokens: 90, outputTokens: 9, totalTokens: 99 },
      },
    ]);
    const { wrapper } = await mountPage();
    await switchTab(wrapper, 'tokens');
    expect(wrapper.find('[aria-label="当前筛选的 Token 用量汇总"]').exists()).toBe(false);
    const summaryCells = () =>
      wrapper
        .get('[aria-label="当前筛选的 Token 用量合计"]')
        .findAll('td')
        .map((cell) => cell.text());
    expect(summaryCells()).toEqual(['合计', '15', '15 / 15', '1,000', '100', '1,100', '73.3']);
    await wrapper.get('.ant-pagination-item-2').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(4);
    expect(summaryCells()).toEqual(['合计', '15', '15 / 15', '1,000', '100', '1,100', '73.3']);
    await choose(wrapper, '站点筛选', 'summary-site-0');
    expect(summaryCells()).toEqual(['合计', '2', '2 / 2', '90', '9', '99', '49.5']);
  });

  it('详情按记录 ID 读取并在左侧关闭的抽屉内展示，不重新查询列表', async () => {
    const { wrapper } = await mountPage();
    const fetch = vi.spyOn(api, 'fetchProbeRecords');
    const detail = vi.spyOn(api, 'fetchProbeRecordDetail');
    await wrapper.get('[aria-label="查看探测记录 probe-001 的详情"]').trigger('click');
    await flushPromises();
    expect(wrapper.getComponent(Drawer).props('open')).toBe(true);
    expect(wrapper.getComponent(Drawer).props('closable')).toMatchObject({ placement: 'start' });
    expect(document.querySelector('.ant-drawer-body')?.textContent).toContain('响应偏慢');
    expect(document.querySelector('.ant-drawer-body')?.textContent).toContain('性能与用量');
    expect(document.querySelector('.ant-drawer-body')?.textContent).toContain(
      'https://input.codes/v1/responses',
    );
    expect(detail).toHaveBeenCalledExactlyOnceWith('probe-001');
    expect(fetch).not.toHaveBeenCalled();
    await new DOMWrapper(document.querySelector('.ant-drawer-close')!).trigger('click');
    await flushPromises();
    expect(wrapper.getComponent(Drawer).props('open')).toBe(false);
  });

  it('删除前显示影响说明；取消不写入，确认后同时刷新列表与统计', async () => {
    const { wrapper } = await mountPage();
    await switchTab(wrapper, 'tokens');
    const before = tokenData(wrapper)!.summary;
    await switchTab(wrapper, 'records');
    const remove = vi.spyOn(api, 'deleteProbeRecord');
    const recordRead = vi.spyOn(api, 'fetchProbeRecords');
    const tokenRead = vi.spyOn(api, 'fetchProbeTokenStats');
    await wrapper.get('[aria-label="删除探测记录 probe-001"]').trigger('click');
    await flushPromises();
    expect(document.querySelector('.ant-modal-body')?.textContent?.trim()).toBe(
      '删除后无法恢复。删除记录时会同步删除对应的缓存文件，并更新对应的 Token 统计。不同步修改对应模型健康状态。',
    );
    expect(remove).not.toHaveBeenCalled();
    await modalButton('取消').trigger('click');
    await flushPromises();
    expect(wrapper.getComponent(Modal).props('open')).toBe(false);
    expect(remove).not.toHaveBeenCalled();
    await wrapper.get('[aria-label="删除探测记录 probe-001"]').trigger('click');
    await flushPromises();
    await modalButton('删除记录').trigger('click');
    await flushPromises();
    expect(remove).toHaveBeenCalledExactlyOnceWith('probe-001');
    expect(recordRead).toHaveBeenCalledOnce();
    expect(tokenRead).toHaveBeenCalledOnce();
    expect(recordData(wrapper)?.total).toBe(29);
    expect(wrapper.find('[data-row-key="probe-001"]').exists()).toBe(false);
    expect(wrapper.getComponent(Modal).props('open')).toBe(false);
    await switchTab(wrapper, 'tokens');
    expect(tokenData(wrapper)?.summary.totalTokens).toBe(before.totalTokens! - 4394);
  });

  it('删除后由页面重查统计，删除前晚到的统计响应不能恢复旧合计', async () => {
    const { wrapper } = await mountPage();
    await switchTab(wrapper, 'tokens');
    const previous = tokenData(wrapper)!;
    await switchTab(wrapper, 'records');
    const pending = deferred<NonNullable<ReturnType<typeof tokenData>>>();
    const read = vi.spyOn(api, 'fetchProbeTokenStats').mockReturnValueOnce(pending.promise);
    await wrapper.get('[aria-label="刷新探测记录和 Token 统计"]').trigger('click');
    await flushPromises();
    await wrapper.get('[aria-label="删除探测记录 probe-001"]').trigger('click');
    await flushPromises();
    await modalButton('删除记录').trigger('click');
    await flushPromises();
    expect(read).toHaveBeenCalledTimes(2);
    await switchTab(wrapper, 'tokens');
    const current = tokenData(wrapper);
    expect(current?.summary.probeCount).toBe(29);
    expect(current?.summary.totalTokens).toBe(previous.summary.totalTokens! - 4394);
    pending.resolve(previous);
    await flushPromises();
    expect(tokenData(wrapper)).toBe(current);
  });

  it('写入等待期间阻止重复确认与关闭，失败保留弹窗和原数据，再次确认可以成功', async () => {
    const { wrapper } = await mountPage();
    const beforeRecords = recordData(wrapper);
    const tokenRead = vi.spyOn(api, 'fetchProbeTokenStats');
    const pending = deferred<void>();
    const remove = vi.spyOn(api, 'deleteProbeRecord').mockReturnValueOnce(pending.promise);
    await wrapper.get('[aria-label="删除探测记录 probe-001"]').trigger('click');
    await flushPromises();
    const modal = wrapper.getComponent(Modal);
    modal.vm.$emit('ok');
    await flushPromises();
    expect(modal.props('confirmLoading')).toBe(true);
    expect(modal.props('closable')).toBe(false);
    expect(modal.props('keyboard')).toBe(false);
    modal.vm.$emit('ok');
    modal.vm.$emit('cancel');
    await flushPromises();
    expect(remove).toHaveBeenCalledOnce();
    expect(modal.props('open')).toBe(true);
    pending.reject(new Error('删除失败'));
    await flushPromises();
    expect(recordData(wrapper)).toBe(beforeRecords);
    expect(tokenRead).not.toHaveBeenCalled();
    expect(modal.props('open')).toBe(true);
    expect(modal.props('confirmLoading')).toBe(false);
    await modalButton('删除记录').trigger('click');
    await flushPromises();
    expect(remove).toHaveBeenCalledTimes(2);
    expect(recordData(wrapper)?.total).toBe(29);
  });

  it('删除尾页最后一条记录后回到前一页；清空时两张表进入真正空态', async () => {
    seedProbeRecords(createProbeRecordFixtures().slice(0, 11));
    const { wrapper } = await mountPage();
    await wrapper.get('.ant-pagination-item-2').trigger('click');
    await flushPromises();
    const last = recordData(wrapper)!.items[0];
    await wrapper.get(`[aria-label="删除探测记录 ${last.id}"]`).trigger('click');
    await flushPromises();
    await modalButton('删除记录').trigger('click');
    await flushPromises();
    expect(recordData(wrapper)?.page).toBe(1);
    expect(wrapper.getComponent(ProbeRecordsTable).props('page')).toBe(1);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    const clear = vi.spyOn(api, 'clearProbeRecords');
    wrapper.getComponent(Dropdown).vm.$emit('menuClick', { key: 'clear' });
    await flushPromises();
    expect(document.querySelector('.ant-modal')?.textContent).toContain('包括当前筛选之外的记录');
    await modalButton('清理全部记录').trigger('click');
    await flushPromises();
    expect(clear).toHaveBeenCalledExactlyOnceWith();
    expect(recordsState.records).toEqual([]);
    expect(wrapper.text()).toContain('暂无探测记录');
    await switchTab(wrapper, 'tokens');
    expect(wrapper.text()).toContain('暂无 Token 统计');
    expect(tokenData(wrapper)?.summary).toEqual({
      probeCount: 0,
      reportedCount: 0,
      totalReportedCount: 0,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      averageTokens: null,
    });
    expect(wrapper.find('[aria-label="当前筛选的 Token 用量合计"]').exists()).toBe(false);
  });

  it('加载、空数据、筛选无结果和读取失败分别展示；失败重试不会被当成空列表', async () => {
    const pending = deferred<RecordsPage<ProbeRecordListItem>>();
    const read = api.fetchProbeRecords;
    vi.spyOn(api, 'fetchProbeRecords').mockReturnValueOnce(pending.promise);
    const { wrapper } = await mountPage({ waitForRecords: false });
    expect(wrapper.get('[aria-label="探测记录列表"]').attributes('aria-busy')).toBe('true');
    expect(wrapper.text()).toContain('数据加载中');
    expect(wrapper.text()).not.toContain('暂无探测记录');
    pending.resolve(
      await read({
        range: '24h',
        siteId: null,
        modelName: null,
        result: null,
        page: 1,
        pageSize: 10,
      }),
    );
    await flushPromises();
    await choose(wrapper, '模型筛选', 'Claude Opus');
    await choose(wrapper, '探测结果筛选', 'slow');
    expect(wrapper.text()).toContain('没有符合条件的探测记录');
    const reset = wrapper.findAll('button').find((button) => button.text() === '重置筛选')!;
    await reset.trigger('click');
    await flushPromises();
    expect(recordData(wrapper)?.total).toBe(30);
    window.history.replaceState({}, '', '/run/records?recordsScenario=records-error');
    await wrapper.get('[aria-label="刷新探测记录和 Token 统计"]').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('探测记录加载失败');
    expect(wrapper.findComponent(ProbeRecordsTable).exists()).toBe(false);
    expect(wrapper.text()).not.toContain('暂无探测记录');
    window.history.replaceState({}, '', '/run/records');
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '重新加载')!
      .trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(ProbeRecordsTable).exists()).toBe(true);
    expect(wrapper.text()).not.toContain('探测记录加载失败');
  });

  it('Token 读取失败不遮蔽正常记录，筛选目录失败不伪装成无限加载', async () => {
    window.history.replaceState({}, '', '/run/records?recordsScenario=tokens-error');
    const { wrapper } = await mountPage();
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    await switchTab(wrapper, 'tokens');
    expect(wrapper.text()).toContain('Token 统计加载失败');
    window.history.replaceState({}, '', '/run/records?recordsScenario=options-error');
    await wrapper.get('[aria-label="刷新探测记录和 Token 统计"]').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('筛选选项加载失败');
    expect(wrapper.text()).not.toMatch(/记录保留|保留时间/);
  });
});
