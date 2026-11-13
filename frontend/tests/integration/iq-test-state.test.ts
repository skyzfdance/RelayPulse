import type { IqSnapshot } from '@/types/domain/iq-test';

import { createPinia, disposePinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { resetEventBusForTests } from '@/api/events';
import * as bankApi from '@/api/iq-test/bank';
import * as historyApi from '@/api/iq-test/history';
import * as snapshotApi from '@/api/iq-test/snapshot';
import * as taskApi from '@/api/iq-test/task';
import { bindFeedbackInstances, resetFeedbackForTests } from '@/composables/useFeedback';
import { mockIqSystemNotificationRequests } from '@/mocks/handlers/config/notifications';
import { seedSites, state as sitesState } from '@/mocks/handlers/config/sites';
import { advanceIqRun, seedIqState } from '@/mocks/handlers/iq-test';
import { getMockHandler, registerMockHandler } from '@/mocks/handlers/registry';
import { operationLogFixtures } from '@/mocks/handlers/run/operation-logs';
import { useIqTestStore } from '@/stores/iq-test';

import { deferred } from '../support/deferred';

let pinia: ReturnType<typeof createPinia>;
const input = {
  modelKey: 'site-1/model-0-0',
  headers: [],
  preset: '高',
  parameters: { reasoning_effort: 'high' },
};
const startInput = {
  questionIds: ['iq-question-1', 'iq-question-2'],
  retryMax: 0,
};
async function readyParticipant() {
  const item = await taskApi.createIqParticipant(input);
  await taskApi.runIqPrecheck(item.snapshotId);
  return item;
}
beforeEach(() => {
  window.history.replaceState({}, '', '/');
  resetEventBusForTests();
  resetFeedbackForTests();
  seedSites();
  seedIqState();
  pinia = createPinia();
  setActivePinia(pinia);
});
afterEach(() => {
  disposePinia(pinia);
  seedIqState();
  vi.useRealTimers();
  window.history.replaceState({}, '', '/');
});

describe('IQ 浏览器契约与状态生命周期', () => {
  it.each([
    ['https://aitogether.site/usage', 'v1/res', 'https://aitogether.site/usagev1/res'],
    ['https://example.test/usage', '/v1/res', 'https://example.test/usage/v1/res'],
    ['https://example.test/usage/', 'v1/res', 'https://example.test/usage/v1/res'],
    ['https://example.test/usage/', '/v1/res', 'https://example.test/usage//v1/res'],
  ])('快照、预检与测试保留拼接地址 %s + %s', async (baseUrl, endpoint, expected) => {
    sitesState.sites[0].baseUrl = baseUrl;
    sitesState.sites[0].connection.probePath = endpoint;
    const participant = await readyParticipant();
    expect(participant.endpoint).toBe(expected);
    expect((await snapshotApi.fetchIqSnapshot()).participants[0].endpoint).toBe(expected);
    const run = await taskApi.startIqRun(startInput);
    expect(run.participants[0].endpoint).toBe(expected);
    for (let tick = 0; tick < 6; tick++) advanceIqRun();
    expect((await historyApi.fetchIqRun(run.id)).participants[0].endpoint).toBe(expected);
  });

  it('快照拒绝需要 URL 纠正的端点，不生成替代展示地址', async () => {
    sitesState.sites[0].connection.probePath = '/../responses';
    await expect(taskApi.createIqParticipant(input)).rejects.toThrow('不会自动纠正');
    expect((await snapshotApi.fetchIqSnapshot()).participants).toHaveLength(0);
  });

  it('题库 CRUD 和启停由接口回环并验证长度', async () => {
    const initial = await snapshotApi.fetchIqSnapshot();
    const saved = await bankApi.saveIqQuestion({
      content: ' 新题目 ',
      referenceAnswer: '',
      enabled: true,
    });
    expect(saved.content).toBe('新题目');
    expect(saved.referenceAnswer).toBeNull();
    await bankApi.setIqQuestionEnabled(saved.id, false);
    const snapshot = await snapshotApi.fetchIqSnapshot();
    expect(snapshot.questionSummary).toEqual({
      total: initial.questionSummary.total + 1,
      enabled: initial.questionSummary.enabled,
    });
    await expect(
      bankApi.saveIqQuestion({ content: ' ', referenceAnswer: null, enabled: true }),
    ).rejects.toThrow();
    await bankApi.deleteIqQuestion(saved.id);
    expect((await snapshotApi.fetchIqSnapshot()).questions).toEqual(initial.questions);
  });
  it('候选只受模型启用影响，停用站点和异常健康模型仍然可选', async () => {
    sitesState.sites[0].enabled = false;
    sitesState.sites[0].models[0].health = 'error';
    const { candidates } = await snapshotApi.fetchIqSnapshot();
    expect(candidates.find((item) => item.modelKey === input.modelKey)?.health).toBe('error');
    expect(candidates.some((item) => item.modelKey === 'site-1/model-0-2')).toBe(false);
  });
  it('添加即快照，start/后续题/重试均不回读或继承源探针配置', async () => {
    const item = await readyParticipant();
    const before = (await snapshotApi.fetchIqSnapshot()).participants[0];
    sitesState.sites[0].name = '源站改名';
    sitesState.sites[0].baseUrl = 'https://changed.invalid';
    sitesState.sites[0].models[0].policy = {
      version: 2,
      headers: [],
      parameters: [{ key: 'max_tokens', type: 'number', value: '8', enabled: true }],
    };
    sitesState.sites[0].models = [];
    const refreshed = await snapshotApi.fetchIqSnapshot();
    expect(refreshed.participants[0]).toEqual(before);
    expect(refreshed.candidates.some((value) => value.modelKey === input.modelKey)).toBe(false);
    const run = await taskApi.startIqRun(startInput);
    expect(run.participants[0].snapshotId).toBe(item.snapshotId);
    expect(run.participants[0].siteName).toBe('OpenAI 主站');
    expect(run.participants[0].parameters).toEqual(input.parameters);
    for (let i = 0; i < 6; i++) advanceIqRun();
    const finished = await historyApi.fetchIqRun(run.id);
    expect(finished.status).toBe('completed');
    expect(finished.participants[0]).toEqual(before);
  });
  it('请求报错按次数重试，重试期间保持运行，耗尽后保留全部尝试', async () => {
    window.history.replaceState({}, '', '/iq-test/task?iqScenario=run-error');
    await readyParticipant();
    const run = await taskApi.startIqRun({ questionIds: ['iq-question-1'], retryMax: 2 });
    for (let tick = 0; tick < 3; tick++) advanceIqRun();
    const retrying = await historyApi.fetchIqRun(run.id);
    expect(retrying.status).toBe('running');
    expect(retrying.entries[0]).toMatchObject({ status: 'running', attempt: 2 });
    for (let tick = 0; tick < 4; tick++) advanceIqRun();
    const finished = await historyApi.fetchIqRun(run.id);
    expect(finished.status).toBe('completed');
    expect(finished.entries[0]).toMatchObject({ status: 'error', attempt: 3 });
    const output = await historyApi.fetchIqEntryOutput(finished.entries[0].id);
    expect(output.attempts.map((attempt) => attempt.status)).toEqual(['error', 'error', 'error']);
  });
  it('题目在start时固化，之后编辑和删除不影响运行', async () => {
    await readyParticipant();
    await bankApi.saveIqQuestion(
      { content: 'start前的新题面', referenceAnswer: 'start前的新答案', enabled: true },
      'iq-question-1',
    );
    const run = await taskApi.startIqRun(startInput);
    await bankApi.deleteIqQuestion('iq-question-1');
    expect((await historyApi.fetchIqRun(run.id)).questions[0]).toMatchObject({
      content: 'start前的新题面',
      referenceAnswer: 'start前的新答案',
    });
  });
  it('失败不阻止参与者添加，重探测才替换快照并读新源配置', async () => {
    window.history.replaceState({}, '', '/?iqScenario=precheck-failure');
    const globalError = vi.fn();
    const feedbackHandlers = {
      success: vi.fn(),
      error: globalError,
      info: vi.fn(),
      warning: vi.fn(),
    };
    bindFeedbackInstances({
      message: feedbackHandlers,
      notification: feedbackHandlers,
      modal: feedbackHandlers,
    });
    const store = useIqTestStore();
    await store.refresh();
    const originalPrecheck = getMockHandler('run_iq_precheck')!;
    registerMockHandler('run_iq_precheck', async (args) => {
      await originalPrecheck(args);
      throw new Error('预检连接失败');
    });
    try {
      const item = await store.addParticipant(input);
      await vi.waitFor(() =>
        expect(store.participants[0].precheck).toMatchObject({
          status: 'unavailable',
        }),
      );
      expect(globalError).not.toHaveBeenCalled();
      registerMockHandler('run_iq_precheck', originalPrecheck);
      await expect(taskApi.startIqRun(startInput)).rejects.toThrow('当前快照');
      sitesState.sites[0].name = '重新探测后的站点';
      const next = await taskApi.replaceIqParticipant(item.id);
      expect(next.snapshotId).not.toBe(item.snapshotId);
      expect(next.siteName).toBe('重新探测后的站点');
      expect((await taskApi.runIqPrecheck(next.snapshotId)).precheck.status).toBe('available');
      expect((await snapshotApi.fetchIqSnapshot()).participants).toHaveLength(1);
    } finally {
      registerMockHandler('run_iq_precheck', originalPrecheck);
    }
  });
  it('同一modelKey的预检结果互不串用，已移除快照的迟到响应不能覆盖新快照', async () => {
    vi.useFakeTimers();
    window.history.replaceState({}, '', '/?iqScenario=precheck-failure&iqPrecheckDelay=100');
    const first = await taskApi.createIqParticipant(input);
    const pending = taskApi.runIqPrecheck(first.snapshotId);
    await vi.advanceTimersByTimeAsync(0);
    await taskApi.removeIqParticipant(first.id);
    const next = await taskApi.createIqParticipant(input);
    const nextPending = taskApi.runIqPrecheck(next.snapshotId);
    await vi.advanceTimersByTimeAsync(100);
    await Promise.all([pending, nextPending]);
    const participants = (await snapshotApi.fetchIqSnapshot()).participants;
    expect(participants).toHaveLength(1);
    expect(participants[0].snapshotId).toBe(next.snapshotId);
    expect(participants[0].precheck.status).toBe('available');
  });
  it('保留字段和嵌套危险路径拒绝写入独立IQ参数', async () => {
    for (const key of ['model', 'messages', 'input', 'prompt', 'stream', 'system']) {
      await expect(
        taskApi.createIqParticipant({ ...input, parameters: { [key]: 'override' } }),
      ).rejects.toThrow('保留字段');
    }
    await expect(
      taskApi.createIqParticipant({
        ...input,
        parameters: JSON.parse('{"nested":{"__proto__":{}}}'),
      }),
    ).rejects.toThrow('不安全');
    expect((await snapshotApi.fetchIqSnapshot()).participants).toHaveLength(0);
  });
  it('同题同时起跑，题目串行，运行时拒绝第二场和删除，取消不回写已结束项', async () => {
    await readyParticipant();
    const second = await taskApi.createIqParticipant({ ...input, modelKey: 'site-2/model-1-0' });
    await taskApi.runIqPrecheck(second.snapshotId);
    const run = await taskApi.startIqRun(startInput);
    expect(run.entries.map((entry) => entry.status)).toEqual([
      'running',
      'running',
      'pending',
      'pending',
    ]);
    expect(run.entries[0].startedAtMs).toBe(run.entries[1].startedAtMs);
    await expect(taskApi.startIqRun(startInput)).rejects.toMatchObject({ code: 'AlreadyRunning' });
    await expect(historyApi.deleteIqRuns([run.id])).rejects.toThrow('进行中');
    for (let i = 0; i < 3; i++) advanceIqRun();
    await taskApi.abortIqRun();
    const aborted = await historyApi.fetchIqRun(run.id);
    expect(aborted.entries.map((entry) => entry.status)).toEqual([
      'completed',
      'aborted',
      'aborted',
      'aborted',
    ]);
    expect(aborted.entries[2]).toMatchObject({
      attempt: 0,
      startedAtMs: null,
      totalTimeMs: null,
      hasRawStream: false,
    });
    await expect(historyApi.openIqRawFile(aborted.entries[2].id)).rejects.toThrow('未发起调用');
    expect(aborted.summary.progressPercent).toBe(100);
  });
  it('正式调用不做本地超时重试，完成后清空草稿并保留历史快照', async () => {
    await readyParticipant();
    window.history.replaceState({}, '', '/?iqScenario=timeout');
    const run = await taskApi.startIqRun({ ...startInput, questionIds: ['iq-question-1'] });
    sitesState.sites = [];
    // 切页会移除场景 query，已发起运行仍使用 start 时的 mock 执行场景。
    window.history.replaceState({}, '', '/iq-test/history');
    for (let i = 0; i < 12; i++) advanceIqRun();
    const finished = await historyApi.fetchIqRun(run.id);
    expect(finished.status).toBe('completed');
    expect(finished.entries[0].status).toBe('completed');
    expect(finished.entries[0].attempt).toBe(1);
    expect((await snapshotApi.fetchIqSnapshot()).participants).toEqual([]);
    expect(mockIqSystemNotificationRequests).toEqual([
      expect.objectContaining({
        ruleId: 'iq.run_completed',
        runId: run.id,
        target: { path: '/iq-test/history', query: { runId: run.id } },
      }),
    ]);
    for (let i = 0; i < 3; i++) advanceIqRun();
    expect(mockIqSystemNotificationRequests).toHaveLength(1);
    expect(finished.participants).toEqual(run.participants);
  });
  it('系统通知使用 IQ 规则模板，混合成功失败的整场只投递一次并携带运行详情目标', async () => {
    const original = await fetchNotificationTemplateDetail('iq.run_completed');
    try {
      await updateNotificationTemplate('iq.run_completed', {
        templateType: 'custom',
        markerId: 'bell',
        titleTemplate: 'IQ 对比已结束',
        bodyTemplate: '请查看本次模型对比详情。',
      });
      await readyParticipant();
      const second = await taskApi.createIqParticipant({ ...input, modelKey: 'site-2/model-1-0' });
      await taskApi.runIqPrecheck(second.snapshotId);
      window.history.replaceState({}, '', '/?iqScenario=run-error');
      const run = await taskApi.startIqRun({ ...startInput, questionIds: ['iq-question-1'] });
      for (let i = 0; i < 5; i++) advanceIqRun();
      expect((await historyApi.fetchIqRun(run.id)).entries.map((entry) => entry.status)).toEqual([
        'error',
        'completed',
      ]);
      expect(mockIqSystemNotificationRequests).toEqual([
        {
          ruleId: 'iq.run_completed',
          runId: run.id,
          markerId: 'bell',
          titleTemplate: 'IQ 对比已结束',
          bodyTemplate: '请查看本次模型对比详情。',
          target: { path: '/iq-test/history', query: { runId: run.id } },
        },
      ]);
    } finally {
      await updateNotificationTemplate('iq.run_completed', original);
    }
  });
  it('删除历史运行后移除记录和详情，同时保留题库与参与者', async () => {
    await readyParticipant();
    const run = await taskApi.startIqRun({ ...startInput, questionIds: ['iq-question-1'] });
    for (let i = 0; i < 3; i++) advanceIqRun();
    const before = await snapshotApi.fetchIqSnapshot();
    expect(before.runs).toHaveLength(1);
    await historyApi.deleteIqRuns([run.id]);
    const after = await snapshotApi.fetchIqSnapshot();
    expect(after.runs).toEqual([]);
    await expect(historyApi.fetchIqRun(run.id)).rejects.toThrow('已不存在');
    expect(after.questions).toEqual(before.questions);
    expect(after.participants).toEqual(before.participants);
  });
  it.each(['global', 'rule'])('关闭%s通知仍正常完成并保存操作记录，不请求提醒', async (mode) => {
    const original = await fetchNotificationSettings();
    try {
      await updateNotificationSettings({
        ...original,
        notificationsEnabled: mode !== 'global',
        rules: original.rules.map((rule) => ({
          ...rule,
          status: mode === 'rule' && rule.id === 'iq.run_completed' ? false : rule.status,
        })),
      });
      await readyParticipant();
      const startLogCount = operationLogFixtures.length;
      const run = await taskApi.startIqRun({ ...startInput, questionIds: ['iq-question-1'] });
      for (let i = 0; i < 3; i++) advanceIqRun();
      const finished = await historyApi.fetchIqRun(run.id);
      expect(finished.status).toBe('completed');
      expect(finished.entries.every((entry) => entry.status === 'completed')).toBe(true);
      expect(finished.summary.progressPercent).toBe(100);
      expect(operationLogFixtures.length).toBe(startLogCount + 1);
      expect(operationLogFixtures[0].action).toBe('iq_run_started');
      expect(mockIqSystemNotificationRequests).toEqual([]);
    } finally {
      await updateNotificationSettings(original);
    }
  });
  it('同revision并发查询只采用最新请求，迟到失败不覆盖最新成功', async () => {
    const snapshot = await snapshotApi.fetchIqSnapshot();
    const store = useIqTestStore();
    await store.refresh();
    const older = deferred<IqSnapshot>();
    const newer = deferred<IqSnapshot>();
    const latest = structuredClone(snapshot);
    latest.candidates[0].siteName = '最新候选源名称';
    vi.spyOn(snapshotApi, 'fetchIqSnapshot')
      .mockImplementationOnce(() => older.promise)
      .mockImplementationOnce(() => newer.promise);
    const first = store.refresh();
    const second = store.refresh();
    newer.resolve(latest);
    await second;
    older.resolve(snapshot);
    await first;
    expect(store.candidates[0].siteName).toBe('最新候选源名称');
    const failed = deferred<IqSnapshot>();
    vi.spyOn(snapshotApi, 'fetchIqSnapshot')
      .mockImplementationOnce(() => failed.promise)
      .mockResolvedValueOnce(latest);
    const pendingFailure = store.refresh();
    await store.refresh();
    failed.reject(new Error('过期失败'));
    await pendingFailure;
    expect(store.error).toBe('');
    expect(store.candidates[0].siteName).toBe('最新候选源名称');
  });
  it('预检命令已返回结果时，同snapshot的旧checking查询不能降级预检结果', async () => {
    const checked = deferred<Awaited<ReturnType<typeof taskApi.runIqPrecheck>>>();
    vi.spyOn(taskApi, 'runIqPrecheck').mockImplementation(() => checked.promise);
    const store = useIqTestStore();
    await store.connect();
    await store.refresh();
    const item = await store.addParticipant(input);
    const checkingSnapshot = await snapshotApi.fetchIqSnapshot();
    const query = deferred<IqSnapshot>();
    vi.spyOn(snapshotApi, 'fetchIqSnapshot').mockImplementationOnce(() => query.promise);
    const pending = store.refresh();
    checked.resolve({
      snapshotId: item.snapshotId,
      precheck: { status: 'available', message: '当前快照已通过', checkedAtMs: Date.now() },
    });
    await vi.waitFor(() => expect(store.participants[0].precheck.status).toBe('available'));
    query.resolve(checkingSnapshot);
    await pending;
    expect(store.participants[0].precheck.status).toBe('available');
  });
  it('store切页后保留当前运行并接收完成，静默预检不写操作记录', async () => {
    const store = useIqTestStore();
    await store.connect();
    await store.refresh();
    const logs = operationLogFixtures.length;
    await store.addParticipant(input);
    await vi.waitFor(() => expect(store.participants[0].precheck.status).toBe('available'));
    expect(operationLogFixtures.length).toBe(logs);
    const run = await store.start({ ...startInput, questionIds: ['iq-question-1'] });
    expect(useIqTestStore().activeRun?.id).toBe(run.id);
    for (let i = 0; i < 3; i++) advanceIqRun();
    await vi.waitFor(() => expect(store.activeRun).toBeNull());
    expect(store.lastRun?.id).toBe(run.id);
    expect(store.runs[0].id).toBe(run.id);
    expect(operationLogFixtures.some((log) => log.action === 'iq_run_started')).toBe(true);
    await store.deleteRuns([run.id]);
    expect(store.runs).toEqual([]);
    expect(store.questions.length).toBeGreaterThan(0);
  });
});
import {
  fetchNotificationSettings,
  fetchNotificationTemplateDetail,
  updateNotificationSettings,
  updateNotificationTemplate,
} from '@/api/config/notifications';
