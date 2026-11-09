import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Select } from 'antdv-next';
import { createPinia, disposePinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as generalApi from '@/api/config/general';
import { resetEventBusForTests } from '@/api/events';
import * as bankApi from '@/api/iq-test/bank';
import * as historyApi from '@/api/iq-test/history';
import * as snapshotApi from '@/api/iq-test/snapshot';
import * as taskApi from '@/api/iq-test/task';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { getMockGeneralSettings } from '@/mocks/handlers/config/general';
import { seedSites, state as sitesState } from '@/mocks/handlers/config/sites';
import { advanceIqRun, seedIqState } from '@/mocks/handlers/iq-test';
import { useIqTestStore } from '@/stores/iq-test';

import OptionCard from '@/components/option-card/OptionCard.vue';
import GeneralView from '@/views/config/general/GeneralView.vue';

const cleanup: (() => void)[] = [];
beforeEach(async () => {
  window.history.replaceState({}, '', '/config/general');
  resetEventBusForTests();
  const pinia = createPinia();
  setActivePinia(pinia);
  cleanup.push(() => disposePinia(pinia));
  seedSites();
  seedIqState();
  await generalApi.updateGeneralSettings({ ...getMockGeneralSettings(), historyRetentionDays: 30 });
  resetFeedbackForTests();
});
afterEach(async () => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  seedIqState();
  await flushPromises();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function mountGeneral() {
  const wrapper = mount(GeneralView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  cleanup.push(() => wrapper.unmount());
  await flushPromises();
  await vi.waitFor(() => expect(wrapper.findAllComponents(OptionCard).length).toBeGreaterThan(0));
  return { wrapper };
}

describe('通用设置中的探针记录保留期', () => {
  it('沿用探针保留时间与现行 30 天选项，经既有设置 API 保存', async () => {
    const save = vi.spyOn(generalApi, 'updateGeneralSettings');
    const { wrapper } = await mountGeneral();
    const card = wrapper
      .findAllComponents(OptionCard)
      .find((item) => item.props('title') === '探测记录保留时间')!;
    expect(card.props('description')).toBe('过期记录会自动清理，不影响站点和模型配置');
    const retention = card.getComponent(Select);
    expect(retention.props('options')).toEqual([{ value: 30, label: '保留 30 天' }]);
    retention.vm.$emit('change', 30);
    await vi.waitFor(() => expect(save).toHaveBeenCalledOnce());
    expect(getMockGeneralSettings().historyRetentionDays).toBe(30);
    expect(save.mock.calls[0]![0].historyRetentionDays).toBe(30);
    expect(wrapper.text()).not.toContain('智商测试数据会自动清理');
    expect(wrapper.text()).not.toContain('容量上限');
    expect(wrapper.text()).not.toContain('超限');
  });

  it('通用设置不提供 IQ 专属管理区，也不请求 IQ 数据', async () => {
    const fetch = vi.spyOn(snapshotApi, 'fetchIqSnapshot');
    const { wrapper } = await mountGeneral();
    const titles = wrapper.findAllComponents(OptionCard).map((card) => card.props('title'));
    expect(titles).not.toContain('智商测试数据');
    expect(wrapper.text()).not.toContain('打开数据目录');
    expect(wrapper.text()).not.toContain('清空智商测试数据');
    expect(fetch).not.toHaveBeenCalled();
    expect(titles).toEqual(expect.arrayContaining(['探测记录保留时间', '诊断日志', '诊断包']));
  });
});

describe('IQ 回放证据手动保留', () => {
  it('时间、探针保留期保存和后续运行不清理证据，手动删除仅移除目标运行', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2025-01-01T00:00:00Z'));
    const participant = await taskApi.createIqParticipant({
      modelKey: 'site-1/model-0-0',
      preset: 'high',
      parameters: { reasoning_effort: 'high' },
    });
    await taskApi.runIqPrecheck(participant.snapshotId);
    const input = {
      questionIds: ['iq-question-1'],
      timeoutSeconds: 120,
      timeoutRetryMax: 5,
    };
    const oldRun = await taskApi.startIqRun(input);
    for (let tick = 0; tick < 3; tick++) advanceIqRun();
    const evidence = await historyApi.fetchIqRun(oldRun.id);
    expect(evidence.status).toBe('completed');
    expect(evidence.entries[0]!.hasRawStream).toBe(true);

    vi.setSystemTime(new Date('2026-11-07T00:00:00Z'));
    // 接口夹具模拟探针保留期从旧值收短，IQ 证据不参与该设置的清理范围。
    await generalApi.updateGeneralSettings({
      ...getMockGeneralSettings(),
      historyRetentionDays: 90,
    });
    await generalApi.updateGeneralSettings({
      ...getMockGeneralSettings(),
      historyRetentionDays: 30,
    });
    sitesState.sites[0]!.name = '后续修改的源站';
    await bankApi.saveIqQuestion(
      { content: '后续修改的题面', referenceAnswer: '后续答案', enabled: true },
      'iq-question-1',
    );
    const nextRun = await taskApi.startIqRun(input);
    for (let tick = 0; tick < 3; tick++) advanceIqRun();
    const nextEvidence = await historyApi.fetchIqRun(nextRun.id);
    const store = useIqTestStore();
    await store.connect();
    await store.refresh();
    expect(store.runs.map((run) => run.id)).toEqual([nextRun.id, oldRun.id]);
    expect(await historyApi.fetchIqRun(oldRun.id)).toEqual(evidence);
    await expect(historyApi.openIqRawFile(evidence.entries[0]!.id)).resolves.toBeUndefined();

    const beforeDelete = await snapshotApi.fetchIqSnapshot();
    await store.deleteRuns([oldRun.id]);
    expect(store.runs.map((run) => run.id)).toEqual([nextRun.id]);
    await expect(historyApi.fetchIqRun(oldRun.id)).rejects.toThrow('已不存在');
    await expect(historyApi.openIqRawFile(evidence.entries[0]!.id)).rejects.toThrow(
      '暂无原始流文件',
    );
    expect(await historyApi.fetchIqRun(nextRun.id)).toEqual(nextEvidence);
    const afterDelete = await snapshotApi.fetchIqSnapshot();
    expect(afterDelete.questions).toEqual(beforeDelete.questions);
    expect(afterDelete.participants).toEqual(beforeDelete.participants);
    await expect(historyApi.openIqRawFile(nextEvidence.entries[0]!.id)).resolves.toBeUndefined();
  });
});
