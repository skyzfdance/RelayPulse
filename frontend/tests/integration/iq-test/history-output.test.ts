import type { IqEntryOutput, IqRun, IqSnapshot } from '@/types/domain/iq-test';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, {
  Cascader,
  DateRangePicker,
  Descriptions,
  Pagination,
  Select,
  Table,
  Tooltip,
} from 'antdv-next';
import { createPinia, disposePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter, RouterView } from 'vue-router';

import * as historyApi from '@/api/iq-test/history';
import * as snapshotApi from '@/api/iq-test/snapshot';
import { useIqTestStore } from '@/stores/iq-test';

import { deferred } from '../../support/deferred';

import IqDetailView from '@/views/iq-test/history/IqDetailView.vue';
import IqHistoryView from '@/views/iq-test/history/IqHistoryView.vue';

vi.mock('@/api/iq-test/history');
vi.mock('@/api/iq-test/snapshot');

function fixture(): IqRun {
  return {
    id: 'history-run',
    status: 'completed',
    startedAtMs: 1794000000000,
    finishedAtMs: 1794000010000,
    questions: [
      {
        id: 'q1',
        content: '已固化题目',
        referenceAnswer: '已保存的参考答案',
        enabled: true,
        createdAtMs: 1,
        updatedAtMs: 1,
      },
    ],
    participants: [
      {
        id: 'participant',
        snapshotId: 'snapshot-original',
        modelKey: 'model-key',
        modelName: '原始模型',
        siteId: 'site',
        siteName: '原始站点',
        protocol: 'openai-compatible',
        health: 'healthy',
        endpoint: 'https://original.example/v1',
        createdAtMs: 1793999999000,
        preset: 'high',
        parameters: { reasoning_effort: 'high' },
        precheck: { status: 'available', message: '', checkedAtMs: 1793999999500 },
      },
    ],
    entries: [
      {
        id: 'entry-one',
        runId: 'history-run',
        questionIndex: 0,
        participantIndex: 0,
        status: 'completed',
        attempt: 1,
        startedAtMs: 1794000000000,
        finishedAtMs: 1794000010000,
        firstTokenMs: 1000,
        totalTimeMs: 10000,
        inputTokens: 10,
        outputTokens: 30,
        errorCode: null,
        errorMessage: null,
        hasRawStream: true,
      },
    ],
    summary: {
      callResult: {
        value: 'all-success',
        success: 1,
        failed: 0,
        timeout: 0,
        aborted: 0,
        notStarted: 0,
      },
      totalEntries: 1,
      terminalEntries: 1,
      successfulEntries: 1,
      progressPercent: 100,
      currentQuestion: 1,
      totalQuestions: 1,
      totalParticipants: 1,
      durationMs: 10000,
      questionProgress: [
        { questionIndex: 0, terminalEntries: 1, totalEntries: 1, status: 'completed' },
      ],
    },
  };
}
function snapshot(runs = [fixture()]): IqSnapshot {
  return {
    revision: 1,
    questions: [],
    questionSummary: { total: 0, enabled: 0 },
    participants: [],
    candidates: [],
    activeRun: null,
    runs,
  };
}
function historyRuns(count: number): IqRun[] {
  return Array.from({ length: count }, (_, index) => {
    const run = structuredClone(fixture());
    run.id = `history-run-${index + 1}`;
    run.status = (index + 1) % 3 === 0 ? 'aborted' : 'completed';
    if (run.status === 'aborted') run.summary.callResult.value = 'aborted';
    run.questions[0]!.content = (index + 1) % 3 === 0 ? '筛选命中题目' : '其他题目';
    run.entries[0]!.id = `${run.id}-entry`;
    run.entries[0]!.runId = run.id;
    return run;
  });
}
const cleanups: (() => void)[] = [];
beforeEach(() => {
  vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot());
  vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockResolvedValue([]);
  vi.mocked(historyApi.fetchIqRun).mockImplementation(async () => fixture());
  vi.mocked(historyApi.deleteIqRuns).mockResolvedValue();
  vi.mocked(historyApi.openIqRawFile).mockResolvedValue();
  vi.mocked(historyApi.fetchIqEntryOutput).mockResolvedValue({
    entryId: 'entry-one',
    attempt: 1,
    attempts: [
      {
        attempt: 1,
        status: 'completed',
        startedAtMs: 1,
        finishedAtMs: 2,
        firstTokenMs: 1,
        totalTimeMs: 1000,
        statusCode: 200,
        errorCode: null,
        bodyComplete: true,
        streamBytes: 100,
        usage: { outputTokens: 30 },
      },
    ],
    parts: [{ kind: 'text', text: '**已保存的回答**' }],
    encryptedReasoning: false,
    parsingComplete: true,
    rawResponse: null,
  });
});
afterEach(() => {
  cleanups
    .splice(0)
    .reverse()
    .forEach((cleanup) => cleanup());
  document.body.innerHTML = '';
  vi.resetAllMocks();
});
async function setup(path: string) {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/iq-test/history', component: IqHistoryView },
      { path: '/iq-test/detail/:id', component: IqDetailView },
      { path: '/iq-test/task', component: { template: '<div>运行看板</div>' } },
    ],
  });
  await router.push(path);
  await router.isReady();
  const wrapper = mount(RouterView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext, pinia, router] },
  });
  cleanups.push(
    () => disposePinia(pinia),
    () => wrapper.unmount(),
  );
  await flushPromises();
  return { wrapper, router, store: useIqTestStore(pinia) };
}
type PageWrapper = Awaited<ReturnType<typeof setup>>['wrapper'];
function rowIds(wrapper: PageWrapper) {
  return wrapper.findAll('tbody tr[data-row-key]').map((row) => row.attributes('data-row-key'));
}
async function paginate(wrapper: PageWrapper, page: number, pageSize: number) {
  wrapper.getComponent(Pagination).vm.$emit('change', page, pageSize);
  await flushPromises();
}
async function updateHistory() {
  void useIqTestStore().refresh();
  await flushPromises();
}
async function confirmRemove(wrapper: PageWrapper, runId: string) {
  await wrapper.get(`[aria-label="删除运行 ${runId}"]`).trigger('click');
  await flushPromises();
  await new DOMWrapper(
    document.querySelector('.ant-popconfirm-buttons button:last-child')!,
  ).trigger('click');
  await flushPromises();
}

describe('IQ 历史本地分页', () => {
  it('默认每页 20 条，独立分页完整切片，翻页将表格滚动归零', async () => {
    const runs = historyRuns(45);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    const { wrapper } = await setup('/iq-test/history');
    const pager = wrapper.getComponent(Pagination);
    const viewport = wrapper.get<HTMLElement>('section[aria-label="IQ 历史记录列表"]');
    expect(wrapper.getComponent(Table).props('pagination')).toBe(false);
    expect(wrapper.getComponent(Table).props('size')).toBe('small');
    expect(viewport.find('.ant-pagination').exists()).toBe(false);
    expect(wrapper.find('[aria-label="重置 IQ 历史筛选"]').exists()).toBe(false);
    expect(wrapper.find('[aria-label="刷新 IQ 历史记录"]').exists()).toBe(false);
    const participantFilter = wrapper.get<HTMLElement>('.ant-select.ant-cascader');
    expect(participantFilter.element.style.width).toBe('320px');
    expect(participantFilter.element.style.maxWidth).toBe('100%');
    expect(participantFilter.element.style.flexShrink).toBe('0');
    expect(pager.props()).toMatchObject({
      current: 1,
      pageSize: 20,
      pageSizeOptions: ['10', '20', '50', '100'],
      showSizeChanger: true,
      showQuickJumper: false,
      size: 'small',
    });
    expect(wrapper.text()).toContain('第 1–20 条，共 45 条');
    const visited = [...rowIds(wrapper)];
    expect(visited).toEqual(runs.slice(0, 20).map((run) => run.id));
    viewport.element.scrollTop = 200;
    await wrapper.get('.ant-pagination-item-2').trigger('click');
    await flushPromises();
    expect(viewport.element.scrollTop).toBe(0);
    expect(rowIds(wrapper)).toEqual(runs.slice(20, 40).map((run) => run.id));
    visited.push(...rowIds(wrapper));
    await paginate(wrapper, 3, 20);
    expect(rowIds(wrapper)).toEqual(runs.slice(40).map((run) => run.id));
    visited.push(...rowIds(wrapper));
    expect(visited).toEqual(runs.map((run) => run.id));
  });

  it('切换每页数量和三项筛选均回到第一页，并对整场结果切片', async () => {
    const runs = historyRuns(45);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    const { wrapper } = await setup('/iq-test/history');
    const pager = wrapper.getComponent(Pagination);
    await paginate(wrapper, 3, 20);
    await paginate(wrapper, 3, 10);
    expect(pager.props('current')).toBe(1);
    expect(pager.props('pageSize')).toBe(10);
    expect(rowIds(wrapper)).toEqual(runs.slice(0, 10).map((run) => run.id));
    await paginate(wrapper, 3, 10);
    const initialRequests = vi.mocked(snapshotApi.fetchIqSnapshot).mock.calls.length;
    wrapper.getComponent(Select).vm.$emit('update:value', 'aborted');
    await flushPromises();
    expect(snapshotApi.fetchIqSnapshot).toHaveBeenCalledTimes(initialRequests + 1);
    const matches = runs.filter((run) => run.status === 'aborted');
    expect(pager.props('current')).toBe(1);
    expect(pager.props('total')).toBe(matches.length);
    expect(rowIds(wrapper)).toEqual(matches.slice(0, 10).map((run) => run.id));
    await paginate(wrapper, 2, 10);
    expect(rowIds(wrapper)).toEqual(matches.slice(10).map((run) => run.id));
    wrapper.getComponent(Cascader).vm.$emit('update:value', [['site']]);
    await flushPromises();
    expect(snapshotApi.fetchIqSnapshot).toHaveBeenCalledTimes(initialRequests + 2);
    expect(pager.props('current')).toBe(1);
    expect(rowIds(wrapper)).toEqual(matches.slice(0, 10).map((run) => run.id));
    await paginate(wrapper, 2, 10);
    const date = new Date(runs[0]!.startedAtMs).toLocaleDateString('sv-SE');
    wrapper.getComponent(DateRangePicker).vm.$emit('change', null, [date, date]);
    await flushPromises();
    expect(snapshotApi.fetchIqSnapshot).toHaveBeenCalledTimes(initialRequests + 3);
    expect(pager.props('current')).toBe(1);
    expect(rowIds(wrapper)).toEqual(matches.slice(0, 10).map((run) => run.id));
    await updateHistory();
    expect(wrapper.getComponent(Select).props('value')).toBe('aborted');
    expect(wrapper.getComponent(Cascader).props('value')).toEqual([['site']]);
    expect(wrapper.getComponent(DateRangePicker).props('value')).toEqual([date, date]);
    wrapper.getComponent(Select).vm.$emit('update:value', undefined);
    wrapper.getComponent(Cascader).vm.$emit('update:value', []);
    wrapper.getComponent(DateRangePicker).vm.$emit('change', null, ['', '']);
    await flushPromises();
    expect(snapshotApi.fetchIqSnapshot).toHaveBeenCalledTimes(initialRequests + 5);
    expect(pager.props('current')).toBe(1);
    expect(pager.props('total')).toBe(45);
    expect(wrapper.getComponent(Cascader).props('value')).toEqual([]);
    expect(wrapper.getComponent(DateRangePicker).props('value')).toEqual([null, null]);
    expect(wrapper.find('[aria-label="搜索 IQ 历史记录"]').exists()).toBe(false);
  });

  it('刷新与删除末页后夹紧页码，空列表显示 0–0 范围', async () => {
    const runs = historyRuns(21);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    const { wrapper } = await setup('/iq-test/history');
    const pager = wrapper.getComponent(Pagination);
    await paginate(wrapper, 2, 20);
    expect(rowIds(wrapper)).toEqual([runs[20]!.id]);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs.slice(0, 20)));
    await updateHistory();
    expect(pager.props('current')).toBe(1);
    expect(rowIds(wrapper)).toEqual(runs.slice(0, 20).map((run) => run.id));
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    await updateHistory();
    await paginate(wrapper, 2, 20);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs.slice(0, 20)));
    await confirmRemove(wrapper, runs[20]!.id);
    expect(historyApi.deleteIqRuns).toHaveBeenCalledExactlyOnceWith([runs[20]!.id]);
    expect(pager.props('current')).toBe(1);
    expect(rowIds(wrapper)).toHaveLength(20);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([]));
    await updateHistory();
    expect(pager.props('current')).toBe(1);
    expect(rowIds(wrapper)).toEqual([]);
    expect(wrapper.text()).toContain('第 0–0 条，共 0 条');
    vi.mocked(snapshotApi.fetchIqSnapshot).mockRejectedValueOnce(new Error('历史数据暂时不可用'));
    await updateHistory();
    expect(wrapper.text()).toContain('历史数据暂时不可用');
    expect(wrapper.findComponent(Pagination).exists()).toBe(false);
  });

  it('加载或删除期间禁用分页并拒绝页码和页大小变化', async () => {
    const runs = historyRuns(45);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    const { wrapper } = await setup('/iq-test/history');
    const pager = wrapper.getComponent(Pagination);
    await paginate(wrapper, 2, 20);
    const loading = deferred<IqSnapshot>();
    vi.mocked(snapshotApi.fetchIqSnapshot).mockReturnValueOnce(loading.promise);
    await updateHistory();
    expect(pager.props('disabled')).toBe(true);
    await paginate(wrapper, 1, 10);
    expect(pager.props('current')).toBe(2);
    expect(pager.props('pageSize')).toBe(20);
    loading.resolve(snapshot(runs));
    await flushPromises();
    expect(pager.props('disabled')).toBe(false);
    const deletion = deferred<void>();
    vi.mocked(historyApi.deleteIqRuns).mockReturnValueOnce(deletion.promise);
    await confirmRemove(wrapper, runs[20]!.id);
    expect(pager.props('disabled')).toBe(true);
    await paginate(wrapper, 1, 10);
    expect(pager.props('current')).toBe(2);
    expect(pager.props('pageSize')).toBe(20);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(
      snapshot(runs.filter((run) => run.id !== runs[20]!.id)),
    );
    deletion.resolve();
    await flushPromises();
    expect(pager.props('disabled')).toBe(false);
    expect(pager.props('current')).toBe(2);
  });
});

describe('IQ 历史表格与独立筛选目录', () => {
  it('目录直接使用独立 API 树，加载和失败不影响历史表格且不回退拼树', async () => {
    const pending = deferred<Awaited<ReturnType<typeof historyApi.fetchIqHistoryFilterOptions>>>();
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockReturnValueOnce(pending.promise);
    const { wrapper } = await setup('/iq-test/history');
    const cascader = wrapper.getComponent(Cascader);
    expect(rowIds(wrapper)).toEqual(['history-run']);
    expect(cascader.props('disabled')).toBe(true);
    expect(cascader.props('options')).toEqual([]);
    pending.reject(new Error('目录服务暂时不可用'));
    await flushPromises();
    expect(wrapper.text()).toContain('目录服务暂时不可用');
    expect(cascader.props('options')).toEqual([]);
    expect(cascader.props('disabled')).toBe(true);
    expect(rowIds(wrapper)).toEqual(['history-run']);
    const independentTree = [
      {
        value: 'server-only-site',
        label: '接口站点',
        children: [{ value: 'server-only-model', label: '接口模型' }],
      },
    ];
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockResolvedValue(independentTree);
    await wrapper.get('[aria-label="重试 IQ 筛选目录"]').trigger('click');
    await flushPromises();
    expect(cascader.props('options')).toEqual(independentTree);
    expect(cascader.props('multiple')).toBe(true);
    expect(cascader.props('disabled')).toBe(false);
    expect(wrapper.text()).not.toContain('目录服务暂时不可用');
    expect(snapshotApi.fetchIqSnapshot).toHaveBeenCalledTimes(1);
  });

  it('展示六列冻结快照，参与者一行摘要、完整信息悬停查看，结果只显示标签', async () => {
    const run = fixture();
    run.questions.push({ ...run.questions[0]!, id: 'q2', content: '第二道完整题面' });
    run.summary.totalQuestions = 2;
    run.summary.totalParticipants = 3;
    run.participants.push(
      {
        ...run.participants[0]!,
        id: 'p2',
        modelKey: 'm2',
        modelName: '第二模型',
        siteName: '第二站点',
        preset: 'low',
      },
      { ...run.participants[0]!, id: 'p3', modelKey: 'm3', modelName: '第三模型' },
    );
    run.entries.push(
      { ...run.entries[0]!, id: 'e2', participantIndex: 1, status: 'error' },
      { ...run.entries[0]!, id: 'e3', participantIndex: 2, status: 'timeout' },
    );
    run.summary.callResult = {
      value: 'partial-failure',
      success: 1,
      failed: 1,
      timeout: 1,
      aborted: 0,
      notStarted: 0,
    };
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([run]));
    const { wrapper } = await setup('/iq-test/history');
    expect(wrapper.findAll('thead th').map((cell) => cell.text())).toEqual([
      '测试时间',
      '测试题目',
      '参与者',
      '调用结果',
      '总耗时',
      '操作',
    ]);
    expect(wrapper.getComponent(Table).props('tableLayout')).toBe('fixed');
    const row = wrapper.get('tbody tr[data-row-key]');
    expect(row.text()).toContain('已固化题目');
    expect(row.text()).toContain('共 2 题');
    expect(row.get('td:nth-child(2) .truncate').text()).toBe('已固化题目');
    expect(row.text()).toContain('原始模型');
    expect(row.text()).not.toContain('原始站点 · 高');
    expect(row.text()).not.toContain('第二模型');
    expect(row.text()).toContain('等 3 位');
    expect(row.text()).not.toContain('第三模型');
    expect(row.text()).not.toContain('snapshot-original');
    expect(row.text()).not.toContain('不渲染的参考答案');
    expect(row.text()).toContain('部分失败');
    expect(row.get('td:nth-child(4)').text()).toBe('部分失败');
    expect(row.get('td:nth-child(4)').findComponent(Tooltip).exists()).toBe(false);
    const questionTooltip = row
      .findAllComponents(Tooltip)
      .find((tooltip: { text: () => string }) => tooltip.text().includes('共 2 题'))!;
    const questionContent = mount({ render: () => questionTooltip.vm.$slots.title?.() });
    expect(questionContent.text()).toContain('已固化题目');
    expect(questionContent.text()).toContain('第二道完整题面');
    questionContent.unmount();
    const participantTooltip = row
      .findAllComponents(Tooltip)
      .find((tooltip: { text: () => string }) => tooltip.text().includes('等 3 位'))!;
    const participantContent = mount({ render: () => participantTooltip.vm.$slots.title?.() });
    expect(participantContent.text()).toContain('第三模型');
    expect(participantContent.text()).toContain('原始站点 · 高');
    expect(participantContent.text()).toContain('第二站点 · 低');
    expect(participantContent.text()).not.toContain('snapshot-original');
    participantContent.unmount();
  });

  it('站点和模型必须同一参与者匹配，多选按 OR 查整场且不缩减结果统计', async () => {
    const run = fixture();
    run.participants.push({
      ...run.participants[0]!,
      id: 'p2',
      siteId: 'other-site',
      siteName: '另一站点',
      modelKey: 'other-model',
      modelName: '另一模型',
    });
    run.entries.push({ ...run.entries[0]!, id: 'e2', participantIndex: 1, status: 'error' });
    run.summary.callResult = {
      value: 'partial-failure',
      success: 1,
      failed: 1,
      timeout: 0,
      aborted: 0,
      notStarted: 0,
    };
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([run]));
    const { wrapper } = await setup('/iq-test/history');
    const cascader = wrapper.getComponent(Cascader);
    cascader.vm.$emit('update:value', [['site', 'other-model']]);
    await flushPromises();
    expect(rowIds(wrapper)).toEqual([]);
    cascader.vm.$emit('update:value', [['missing-site'], ['other-site', 'other-model']]);
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['history-run']);
    expect(wrapper.get('tbody').text()).toContain('原始模型');
    expect(wrapper.get('tbody tr[data-row-key] td:nth-child(4)').text()).toBe('部分失败');
    wrapper.getComponent(Select).vm.$emit('update:value', 'all-success');
    await flushPromises();
    expect(rowIds(wrapper)).toEqual([]);
    wrapper.getComponent(Select).vm.$emit('update:value', 'partial-failure');
    cascader.vm.$emit('update:value', [['site']]);
    await flushPromises();
    expect(rowIds(wrapper)).toEqual(['history-run']);
    wrapper.getComponent(DateRangePicker).vm.$emit('change', null, ['2000-01-01', '2000-01-02']);
    await flushPromises();
    expect(rowIds(wrapper)).toEqual([]);
  });

  it('刷新失败保留筛选，重试恢复目录；删除与新终态记录均使目录更新', async () => {
    const tree = [
      { value: 'site', label: '旧站点', children: [{ value: 'model-key', label: '原始模型' }] },
    ];
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockResolvedValue(tree);
    const { wrapper, store } = await setup('/iq-test/history');
    const cascader = wrapper.getComponent(Cascader);
    cascader.vm.$emit('update:value', [['site']]);
    await flushPromises();
    expect(historyApi.fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(1);
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockRejectedValueOnce(
      new Error('目录刷新失败'),
    );
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(
      snapshot([fixture(), { ...fixture(), id: 'new-terminal-run' }]),
    );
    await updateHistory();
    expect(cascader.props('value')).toEqual([['site']]);
    expect(cascader.props('options')).toEqual(tree);
    expect(cascader.props('disabled')).toBe(true);
    expect(rowIds(wrapper)).toEqual(['history-run', 'new-terminal-run']);
    await wrapper.get('[aria-label="重试 IQ 筛选目录"]').trigger('click');
    await flushPromises();
    expect(cascader.props('value')).toEqual([['site']]);
    expect(historyApi.fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(3);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([]));
    await confirmRemove(wrapper, 'history-run');
    expect(historyApi.fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(4);
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([fixture()]));
    await store.refresh();
    await flushPromises();
    expect(historyApi.fetchIqHistoryFilterOptions).toHaveBeenCalledTimes(5);
    expect(cascader.props('value')).toEqual([['site']]);
  });

  it('模型勾选使站点显示半选，目录变化保持具体路径，明确选父级才全选', async () => {
    const runs = historyRuns(2);
    runs[1]!.participants[0]!.modelKey = 'new-model';
    runs[1]!.participants[0]!.modelName = '新增模型';
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot(runs));
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockResolvedValue([
      { value: 'site', label: '原始站点', children: [{ value: 'model-key', label: '原始模型' }] },
    ]);
    const { wrapper } = await setup('/iq-test/history');
    const cascader = wrapper.getComponent(Cascader);
    expect(cascader.props('checkStrictly')).toBe(true);
    await cascader.get('input').trigger('mousedown');
    await flushPromises();
    const site = () =>
      new DOMWrapper(
        document.querySelector<HTMLElement>('.ant-cascader-menu-item[title="原始站点"]')!,
      );
    await site().get('.ant-cascader-menu-item-content').trigger('click');
    await flushPromises();
    await new DOMWrapper(
      document.querySelector<HTMLElement>(
        '.ant-cascader-menu-item[title="原始模型"] .ant-cascader-checkbox',
      )!,
    ).trigger('click');
    await flushPromises();
    expect(cascader.props('value')).toEqual([['site', 'model-key']]);
    expect(rowIds(wrapper)).toEqual([runs[0]!.id]);
    expect(site().attributes('aria-checked')).toBe('mixed');
    expect(site().get('.ant-cascader-checkbox').classes()).toContain(
      'ant-cascader-checkbox-indeterminate',
    );
    const modelCheckbox = () =>
      new DOMWrapper(
        document.querySelector<HTMLElement>(
          '.ant-cascader-menu-item[title="原始模型"] .ant-cascader-checkbox',
        )!,
      );
    await modelCheckbox().trigger('click');
    await flushPromises();
    expect(cascader.props('value')).toEqual([]);
    expect(site().attributes('aria-checked')).toBe('false');
    expect(site().get('.ant-cascader-checkbox').classes()).not.toContain(
      'ant-cascader-checkbox-indeterminate',
    );
    await modelCheckbox().trigger('click');
    await flushPromises();
    expect(site().attributes('aria-checked')).toBe('mixed');
    vi.mocked(historyApi.fetchIqHistoryFilterOptions).mockResolvedValue([
      {
        value: 'site',
        label: '原始站点',
        children: [
          { value: 'model-key', label: '原始模型' },
          { value: 'new-model', label: '新增模型' },
        ],
      },
    ]);
    const newRun = { ...runs[1]!, id: 'new-terminal-run' };
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([...runs, newRun]));
    await updateHistory();
    expect(cascader.props('value')).toEqual([['site', 'model-key']]);
    expect(rowIds(wrapper)).toEqual([runs[0]!.id]);
    expect(site().attributes('aria-checked')).toBe('mixed');
    wrapper.getComponent(Select).vm.$emit('update:value', undefined);
    wrapper.getComponent(Cascader).vm.$emit('update:value', []);
    wrapper.getComponent(DateRangePicker).vm.$emit('change', null, ['', '']);
    await flushPromises();
    await cascader.get('input').trigger('mousedown');
    await flushPromises();
    await site().get('.ant-cascader-checkbox').trigger('click');
    await flushPromises();
    expect(cascader.props('value')).toEqual([['site']]);
    expect(site().attributes('aria-checked')).toBe('true');
    expect(site().get('.ant-cascader-checkbox').classes()).not.toContain(
      'ant-cascader-checkbox-indeterminate',
    );
    expect(rowIds(wrapper)).toEqual([...runs, newRun].map((run) => run.id));
  });
});

describe('IQ 历史与回放来源', () => {
  it('历史只显示终态运行，详情输出导航后返回恢复同一运行抽屉', async () => {
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(
      snapshot([fixture(), { ...fixture(), id: 'active-run', status: 'running' }]),
    );
    const { wrapper, router } = await setup('/iq-test/history');
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(1);
    const detailButton = wrapper.get('[aria-label="查看运行 history-run 的详情"]');
    expect(detailButton.classes()).toContain('ant-btn-color-primary');
    expect(detailButton.classes()).toContain('ant-btn-variant-text');
    await detailButton.trigger('click');
    await flushPromises();
    expect(document.querySelector('.ant-drawer-header')?.textContent).toContain('运行详情');
    expect(document.querySelector('.ant-drawer-header')?.textContent).toContain('1 道题目');
    expect(document.querySelector('.ant-drawer-body')?.textContent).toContain('已固化题目');
    expect(document.querySelector('.ant-drawer-body')?.textContent).not.toContain(
      'snapshot-original',
    );
    const outputButton = new DOMWrapper(
      document.querySelector('[aria-label="查看输出 entry-one"]')!,
    );
    expect(outputButton.classes()).toContain('ant-btn-color-primary');
    expect(outputButton.classes()).toContain('ant-btn-variant-text');
    await outputButton.trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/iq-test/detail/entry-one');
    expect(router.currentRoute.value.query).toEqual({ from: 'history', runId: 'history-run' });
    expect(wrapper.text()).toContain('来自历史记录');
    await wrapper.get('[aria-label="返回来源页面"]').trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/iq-test/history');
    expect(router.currentRoute.value.query.runId).toBe('history-run');
    expect(document.querySelector('.ant-drawer-body')?.textContent).toContain('原始模型');
  });

  it('回放使用描述列表展示冻结快照，点击气泡查看完整长参数，不读取源配置', async () => {
    const run = fixture();
    run.participants[0]!.parameters = {
      reasoning_effort: 'high',
      options: { description: '完整参数'.repeat(1500), flags: [true, false, null] },
      last_parameter: '末尾字段保留',
    };
    vi.mocked(historyApi.fetchIqRun).mockResolvedValue(run);
    const { wrapper } = await setup('/iq-test/detail/entry-one?from=history&runId=history-run');
    expect(historyApi.fetchIqRun).toHaveBeenCalledExactlyOnceWith('history-run');
    expect(snapshotApi.fetchIqSnapshot).not.toHaveBeenCalled();
    expect(wrapper.get('[aria-label="实际使用的参与者快照"]').text()).toContain('原始站点');
    expect(wrapper.text()).toContain('https://original.example/v1');
    expect(wrapper.text()).not.toContain('snapshot-original');
    expect(wrapper.getComponent(Descriptions).props()).toMatchObject({
      bordered: true,
      size: 'small',
      column: { xs: 1, sm: 2 },
    });
    expect(wrapper.get('[aria-label="实际使用的参与者快照"]').find('pre').exists()).toBe(false);
    expect(document.querySelector('[aria-label="完整 IQ 请求参数"]')).toBeNull();
    await wrapper.get('[aria-label="查看完整 IQ 请求参数"]').trigger('click');
    await flushPromises();
    const parameterContent = document.querySelector<HTMLElement>(
      '[aria-label="完整 IQ 请求参数"]',
    )!;
    expect(parameterContent).not.toBeNull();
    expect(JSON.parse(parameterContent.textContent!)).toEqual(run.participants[0]!.parameters);
    expect(parameterContent.textContent).toContain('末尾字段保留');
    expect(parameterContent.classList.contains('overflow-auto')).toBe(true);
    expect(wrapper.text()).toContain('已保存的参考答案');
    expect(wrapper.text()).toContain('已保存的回答');
    expect(wrapper.text()).not.toContain('回放正文留空');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('打开记录目录'))!
      .trigger('click');
    await flushPromises();
    expect(historyApi.openIqRawFile).toHaveBeenCalledExactlyOnceWith('entry-one', 1);
  });

  it('切换旧尝试时显示 loading，按该次状态和内容回放并打开对应原始流', async () => {
    const latest: IqEntryOutput = {
      entryId: 'entry-one',
      attempt: 2,
      attempts: [1, 2].map((attempt) => ({
        attempt,
        status: attempt === 1 ? 'timeout' : 'completed',
        startedAtMs: attempt * 1000,
        finishedAtMs: attempt * 2000,
        firstTokenMs: null,
        totalTimeMs: 1000,
        statusCode: 200,
        errorCode: attempt === 1 ? 'output_timeout' : null,
        bodyComplete: attempt === 2,
        streamBytes: 100,
        usage: {},
      })),
      parts: [{ kind: 'text', text: '最后一次完整回答' }],
      encryptedReasoning: false,
      parsingComplete: true,
      rawResponse: null,
    };
    const earlier = deferred<IqEntryOutput>();
    vi.mocked(historyApi.fetchIqEntryOutput)
      .mockResolvedValueOnce(latest)
      .mockReturnValueOnce(earlier.promise);
    const { wrapper } = await setup('/iq-test/detail/entry-one?from=history&runId=history-run');
    expect(wrapper.text()).toContain('最后一次完整回答');
    wrapper.getComponent(Select).vm.$emit('change', 1);
    await flushPromises();
    expect(wrapper.find('.ant-spin').exists()).toBe(true);
    expect(wrapper.text()).not.toContain('最后一次完整回答');
    earlier.resolve({
      ...latest,
      attempt: 1,
      parts: [],
      encryptedReasoning: true,
    });
    await flushPromises();
    expect(historyApi.fetchIqEntryOutput).toHaveBeenLastCalledWith('entry-one', 1);
    expect(wrapper.get('h2').text()).toContain('超时');
    expect(wrapper.text()).toContain('本次包含加密推理数据');
    expect(wrapper.text()).toContain('本次尝试未收到可展示的回答正文');
    await wrapper
      .findAll('button')
      .find((button) => button.text().includes('打开记录目录'))!
      .trigger('click');
    await flushPromises();
    expect(historyApi.openIqRawFile).toHaveBeenCalledExactlyOnceWith('entry-one', 1);
  });

  it('未发起调用的中止项保留骨架，禁用原始流入口，运行来源返回看板', async () => {
    const run = fixture();
    run.status = 'aborted';
    run.entries[0] = { ...run.entries[0]!, status: 'aborted', attempt: 0, hasRawStream: false };
    vi.mocked(historyApi.fetchIqRun).mockResolvedValue(run);
    const { wrapper, router } = await setup('/iq-test/detail/entry-one?from=run&runId=history-run');
    const raw = wrapper.findAll('button').find((button) => button.text().includes('打开记录目录'))!;
    expect(raw.attributes('disabled')).toBeDefined();
    await raw.trigger('click');
    expect(historyApi.openIqRawFile).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('来自运行看板');
    await wrapper.get('[aria-label="返回来源页面"]').trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/iq-test/task');
  });

  it('不接受与运行不匹配的调用标识，旧详情晚到不覆盖新来源', async () => {
    const old = deferred<IqRun>();
    vi.mocked(historyApi.fetchIqRun).mockReturnValueOnce(old.promise);
    const { wrapper, router } = await setup('/iq-test/detail/entry-one?from=run&runId=history-run');
    await router.push('/iq-test/detail/foreign-entry?from=history&runId=another-run');
    await flushPromises();
    expect(wrapper.text()).toContain('这条调用不存在或不属于当前运行');
    old.resolve(fixture());
    await flushPromises();
    expect(wrapper.text()).toContain('这条调用不存在或不属于当前运行');
    expect(wrapper.find('[aria-label="实际使用的参与者快照"]').exists()).toBe(false);
  });

  it('历史仅提供逐条删除，取消不写入，确认后仅移除目标记录', async () => {
    const otherRun = { ...fixture(), id: 'other-run' };
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([fixture(), otherRun]));
    const { wrapper } = await setup('/iq-test/history');
    expect(wrapper.findAll('input[type="checkbox"]')).toHaveLength(0);
    expect(wrapper.text()).not.toContain('删除所选');
    expect(wrapper.text()).not.toContain('清空');
    await wrapper.get('[aria-label="删除运行 history-run"]').trigger('click');
    await flushPromises();
    expect(document.querySelector('.ant-popconfirm')?.textContent).toContain(
      '将同时删除原始流文件与产物，不可恢复',
    );
    expect(historyApi.deleteIqRuns).not.toHaveBeenCalled();
    const cancel = document.querySelector('.ant-popconfirm-buttons button')!;
    await new DOMWrapper(cancel).trigger('click');
    expect(historyApi.deleteIqRuns).not.toHaveBeenCalled();
    await wrapper.get('[aria-label="删除运行 history-run"]').trigger('click');
    await flushPromises();
    vi.mocked(snapshotApi.fetchIqSnapshot).mockResolvedValue(snapshot([otherRun]));
    await new DOMWrapper(
      document.querySelector('.ant-popconfirm-buttons button:last-child')!,
    ).trigger('click');
    await flushPromises();
    expect(historyApi.deleteIqRuns).toHaveBeenCalledExactlyOnceWith(['history-run']);
    expect(wrapper.find('[aria-label="删除运行 history-run"]').exists()).toBe(false);
    expect(wrapper.find('[aria-label="删除运行 other-run"]').exists()).toBe(true);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(1);
  });
});
