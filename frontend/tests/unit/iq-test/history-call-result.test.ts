import type { IqCallResult, IqEntry, IqRun } from '@/types/domain/iq-test';

import { describe, expect, it } from 'vitest';

import { summarizeIqCallResult } from '@/mocks/handlers/iq-test-history';

function entry(status: IqEntry['status'], attempt = 1): IqEntry {
  return {
    id: 'entry',
    runId: 'run',
    questionIndex: 0,
    participantIndex: 0,
    status,
    attempt,
    startedAtMs: attempt > 0 ? 1_000 : null,
    finishedAtMs: attempt > 0 ? 2_000 : null,
    firstTokenMs: null,
    totalTimeMs: attempt > 0 ? 1_000 : null,
    inputTokens: null,
    outputTokens: null,
    errorCode: null,
    errorMessage: null,
    hasRawStream: attempt > 0,
  };
}

describe('IQ Mock 后端调用结果汇总', () => {
  it.each([
    { statuses: ['completed', 'completed'], value: 'all-success' },
    { statuses: ['completed', 'error'], value: 'partial-failure' },
    { statuses: ['completed', 'timeout'], value: 'partial-failure' },
    { statuses: ['error'], value: 'all-failure' },
    { statuses: ['timeout'], value: 'all-failure' },
    { statuses: ['error', 'timeout'], value: 'all-failure' },
  ] satisfies { statuses: IqEntry['status'][]; value: IqCallResult['value'] }[])(
    '已完成场次 $statuses 汇总为 $value',
    ({ statuses, value }) => {
      const source: Pick<IqRun, 'status' | 'entries'> = {
        status: 'completed',
        entries: statuses.map((status) => entry(status)),
      };
      const original = structuredClone(source);
      expect(summarizeIqCallResult(source).value).toBe(value);
      expect(source).toEqual(original);
    },
  );

  it('中止优先，失败与超时单列，重试仍只统计一次调用', () => {
    const source: Pick<IqRun, 'status' | 'entries'> = {
      status: 'aborted',
      entries: [
        entry('completed'),
        entry('error', 3),
        entry('timeout', 6),
        entry('aborted', 2),
        entry('aborted', 1),
        entry('aborted', 0),
        entry('aborted', 0),
      ],
    };

    expect(summarizeIqCallResult(source)).toEqual({
      value: 'aborted',
      success: 1,
      failed: 1,
      timeout: 1,
      aborted: 2,
      notStarted: 2,
    });
    expect(source.status).toBe('aborted');
    expect(source.entries.map((item) => item.attempt)).toEqual([1, 3, 6, 2, 1, 0, 0]);
  });

  it('即使已完成调用全部成功，整场中止仍为中止结果', () => {
    expect(summarizeIqCallResult({ status: 'aborted', entries: [entry('completed')] })).toEqual({
      value: 'aborted',
      success: 1,
      failed: 0,
      timeout: 0,
      aborted: 0,
      notStarted: 0,
    });
  });

  it('不将运行中或等待中的条目统计为已结束调用', () => {
    expect(
      summarizeIqCallResult({
        status: 'running',
        entries: [entry('completed'), entry('running'), entry('pending', 0)],
      }),
    ).toEqual({
      value: 'all-success',
      success: 1,
      failed: 0,
      timeout: 0,
      aborted: 0,
      notStarted: 0,
    });
  });
});
