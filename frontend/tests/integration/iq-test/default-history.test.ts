import type { IqRun } from '@/types/domain/iq-test';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as bankApi from '@/api/iq-test/bank';
import * as historyApi from '@/api/iq-test/history';
import * as snapshotApi from '@/api/iq-test/snapshot';
import * as taskApi from '@/api/iq-test/task';
import { mockIqSystemNotificationRequests } from '@/mocks/handlers/config/notifications';
import { seedSites } from '@/mocks/handlers/config/sites';
import { advanceIqRun, iqState, seedDefaultIqHistory, seedIqState } from '@/mocks/handlers/iq-test';
import { operationLogFixtures } from '@/mocks/handlers/run/operation-logs';

const startupHistoryIds = iqState.runs.map((run) => run.id);

beforeEach(() => {
  window.history.replaceState({}, '', '/');
  seedSites();
  seedIqState();
});

afterEach(() => {
  seedIqState();
  vi.useRealTimers();
  window.history.replaceState({}, '', '/');
});

function findRun(runs: IqRun[], id: string) {
  const run = runs.find((item) => item.id === id);
  expect(run).toBeDefined();
  return run!;
}

describe('IQ 默认历史夹具', () => {
  it('浏览器 mock 默认启动即注入历史样例', () => {
    expect(startupHistoryIds).toEqual([
      'iq-fixture-run-all-success',
      'iq-fixture-run-mixed',
      'iq-fixture-run-timeout',
      'iq-fixture-run-aborted',
    ]);
  });

  it('显式 seedIqState 仍清空历史与参与者，并保留题库', async () => {
    const snapshot = await snapshotApi.fetchIqSnapshot();

    expect(snapshot.runs).toEqual([]);
    expect(snapshot.participants).toEqual([]);
    expect(snapshot.questions).toHaveLength(24);

    seedIqState(true);
    expect((await snapshotApi.fetchIqSnapshot()).questions).toEqual([]);
  });

  it('当前题库涵盖多页、代码换行和启停状态，编辑不改变独立历史题目快照', async () => {
    seedDefaultIqHistory();
    const snapshot = await snapshotApi.fetchIqSnapshot();
    expect(snapshot.questionSummary).toEqual({ total: 24, enabled: 19 });
    expect(new Set(snapshot.questions.map((question) => question.id)).size).toBe(24);
    expect(snapshot.questions.slice(10, 20)).toHaveLength(10);
    expect(snapshot.questions.slice(20)).toHaveLength(4);
    expect(snapshot.questions.some((question) => question.content.includes('\n\nfunction'))).toBe(
      true,
    );
    expect(snapshot.questions.some((question) => question.content.includes('\nSELECT'))).toBe(true);
    expect(snapshot.questions.some((question) => question.content.length < 40)).toBe(true);
    expect(snapshot.questions.some((question) => question.content.length > 120)).toBe(true);
    expect(snapshot.questions.some((question) => question.referenceAnswer === null)).toBe(true);
    expect(snapshot.questions.some((question) => question.referenceAnswer !== null)).toBe(true);
    expect(snapshot.questions[3]).toMatchObject({ id: 'iq-question-4', enabled: false });
    await bankApi.saveIqQuestion(
      { content: '当前题库的修改', referenceAnswer: null, enabled: false },
      'iq-question-1',
    );
    const after = await snapshotApi.fetchIqSnapshot();
    expect(after.runs).toEqual(snapshot.runs);
    expect(after.runs[0]!.questions[0]!.id).toBe('iq-fixture-question-logic');
  });

  it('注入稳定终态样例，不发通知或操作记录，也不启动运行', async () => {
    const operationCount = operationLogFixtures.length;
    vi.useFakeTimers();
    expect(vi.getTimerCount()).toBe(0);
    seedDefaultIqHistory();
    expect(vi.getTimerCount()).toBe(0);
    const snapshot = await snapshotApi.fetchIqSnapshot();
    const runs = snapshot.runs;

    expect(runs.map((run) => [run.id, run.status])).toEqual([
      ['iq-fixture-run-all-success', 'completed'],
      ['iq-fixture-run-mixed', 'completed'],
      ['iq-fixture-run-timeout', 'completed'],
      ['iq-fixture-run-aborted', 'aborted'],
    ]);
    expect(snapshot.activeRun).toBeNull();
    expect(runs.every((run) => run.entries.every((entry) => entry.status !== 'running'))).toBe(
      true,
    );
    expect(
      findRun(runs, 'iq-fixture-run-all-success').entries.every(
        (entry) => entry.status === 'completed',
      ),
    ).toBe(true);
    expect(new Set(runs.map((run) => run.summary.durationMs)).size).toBe(4);
    expect(
      findRun(runs, 'iq-fixture-run-all-success').entries.every(
        (entry) => entry.outputTokens !== null,
      ),
    ).toBe(true);

    const mixed = findRun(runs, 'iq-fixture-run-mixed');
    expect(mixed.entries.some((entry) => entry.status === 'completed')).toBe(true);
    expect(mixed.entries.some((entry) => entry.status === 'error')).toBe(true);
    expect(mixed.entries.some((entry) => entry.errorMessage?.includes('HTTP 502'))).toBe(true);
    expect(mixed.questions).toHaveLength(3);
    expect(mixed.participants).toHaveLength(3);

    const timeout = findRun(runs, 'iq-fixture-run-timeout').entries[0]!;
    expect(timeout).toMatchObject({
      status: 'timeout',
      attempt: 6,
      errorCode: 'output_timeout',
      outputTokens: null,
      hasRawStream: true,
    });

    const aborted = findRun(runs, 'iq-fixture-run-aborted');
    expect(aborted.entries.some((entry) => entry.status === 'completed')).toBe(true);
    expect(aborted.entries.some((entry) => entry.status === 'aborted' && entry.attempt > 0)).toBe(
      true,
    );
    expect(aborted.entries.some((entry) => entry.status === 'aborted' && entry.attempt === 0)).toBe(
      true,
    );

    const retired = mixed.participants.find(
      (participant) => participant.id === 'iq-fixture-participant-retired',
    );
    expect(retired).toMatchObject({
      modelKey: 'retired-site/retired-model',
      headers: [],
      preset: 'custom',
      parameters: { reasoning_effort: 'low' },
    });
    expect(snapshot.candidates.some((candidate) => candidate.modelKey === retired?.modelKey)).toBe(
      false,
    );
    expect(mockIqSystemNotificationRequests).toEqual([]);
    expect(operationLogFixtures).toHaveLength(operationCount);
    expect(iqState.runs).toHaveLength(4);
  });

  it('清空默认历史后仍可添加参与者并正常完成新测试', async () => {
    seedDefaultIqHistory();
    seedIqState();
    const participant = await taskApi.createIqParticipant({
      modelKey: 'site-1/model-0-0',
      headers: [],
      preset: 'high',
      parameters: { reasoning_effort: 'high' },
    });
    await taskApi.runIqPrecheck(participant.snapshotId);
    const run = await taskApi.startIqRun({
      questionIds: ['iq-question-1'],
      retryMax: 0,
    });

    expect(run.status).toBe('running');
    for (let index = 0; index < 3; index++) advanceIqRun();
    expect((await historyApi.fetchIqRun(run.id)).status).toBe('completed');
  });
});
