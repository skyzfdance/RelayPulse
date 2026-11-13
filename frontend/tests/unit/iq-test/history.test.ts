import type { IqCallResult, IqEntry, IqParticipant, IqRun } from '@/types/domain/iq-test';
import type { IqHistoryFilters } from '@/views/iq-test/history/iq-history';

import { describe, expect, it } from 'vitest';

import { filterIqHistory, getIqHistoryResult } from '@/views/iq-test/history/iq-history';

function entry(status: IqEntry['status'], attempt = 1): IqEntry {
  return {
    id: 'entry',
    runId: 'run',
    questionIndex: 0,
    participantIndex: 0,
    status,
    attempt,
    startedAtMs: attempt ? 1_000 : null,
    finishedAtMs: attempt ? 2_000 : null,
    firstTokenMs: null,
    totalTimeMs: attempt ? 1_000 : null,
    inputTokens: null,
    outputTokens: null,
    errorCode: null,
    errorMessage: null,
    hasRawStream: attempt > 0,
  };
}

function participant(siteId: string, modelKey: string): IqParticipant {
  return {
    id: `${siteId}/${modelKey}`,
    snapshotId: `${siteId}/${modelKey}/snapshot`,
    siteId,
    siteName: '相同站点显示名',
    modelKey,
    modelName: '相同模型显示名',
    protocol: 'openai-compatible',
    health: 'healthy',
    endpoint: 'https://example.invalid/v1',
    createdAtMs: 500,
    headers: [],
    preset: 'high',
    parameters: { reasoning_effort: 'high' },
    precheck: { status: 'available', message: '', checkedAtMs: 600 },
  };
}

function run(patch: Partial<IqRun> = {}, callResult: Partial<IqCallResult> = {}): IqRun {
  return {
    id: 'run',
    status: 'completed',
    questions: [],
    participants: [participant('site-1', 'model-1')],
    entries: [entry('completed')],
    startedAtMs: 1_000,
    finishedAtMs: 2_000,
    summary: {
      callResult: {
        value: 'all-success',
        success: 1,
        failed: 0,
        timeout: 0,
        aborted: 0,
        notStarted: 0,
        ...callResult,
      },
      totalEntries: 1,
      terminalEntries: 1,
      successfulEntries: 1,
      progressPercent: 100,
      currentQuestion: 1,
      totalQuestions: 1,
      totalParticipants: 1,
      durationMs: 1_000,
      questionProgress: [],
    },
    ...patch,
  };
}

function filters(patch: Partial<IqHistoryFilters> = {}): IqHistoryFilters {
  return { startAtMs: null, endAtMs: null, participants: [], result: '', ...patch };
}

describe('IQ 历史调用结果展示', () => {
  it.each([
    { value: 'all-success', label: '全部成功', color: 'success' },
    { value: 'partial-failure', label: '部分失败', color: 'warning' },
    { value: 'all-failure', label: '全部失败', color: 'error' },
    { value: 'aborted', label: '已中止', color: 'warning' },
  ] satisfies { value: IqCallResult['value']; label: string; color: string }[])(
    '后端分类 $value 仅映射为 $label 的标签和颜色',
    ({ value, label, color }) => {
      const source = run({}, { value });
      expect(getIqHistoryResult(source)).toEqual({ ...source.summary.callResult, label, color });
    },
  );

  it('展示直接消费后端分类和全部计数，即使 entries 不同也不重新汇总', () => {
    const source = run(
      { entries: [entry('completed')] },
      { value: 'aborted', success: 3, failed: 4, timeout: 5, aborted: 6, notStarted: 7 },
    );
    expect(getIqHistoryResult(source)).toEqual({
      value: 'aborted',
      label: '已中止',
      color: 'warning',
      success: 3,
      failed: 4,
      timeout: 5,
      aborted: 6,
      notStarted: 7,
    });
  });
});

describe('IQ 历史筛选', () => {
  it('按开始时间使用包含两端的范围，默认不限时间且不显示运行中记录', () => {
    const sources = [
      run({ id: 'before', startedAtMs: 999 }),
      run({ id: 'start', startedAtMs: 1_000 }),
      run({ id: 'end', startedAtMs: 2_000 }),
      run({ id: 'after', startedAtMs: 2_001 }),
      run({ id: 'active', status: 'running', startedAtMs: 1_500 }),
    ];
    expect(filterIqHistory(sources, filters()).map((item) => item.id)).toEqual([
      'before',
      'start',
      'end',
      'after',
    ]);
    expect(
      filterIqHistory(sources, filters({ startAtMs: 1_000, endAtMs: 2_000 })).map(
        (item) => item.id,
      ),
    ).toEqual(['start', 'end']);
    expect(filterIqHistory(sources, filters({ startAtMs: 2_001 })).map((item) => item.id)).toEqual([
      'after',
    ]);
    expect(filterIqHistory(sources, filters({ endAtMs: 999 })).map((item) => item.id)).toEqual([
      'before',
    ]);
  });

  it('选择站点包含其全部历史模型，具体模型使用稳定标识匹配，多项为 OR', () => {
    const sources = [
      run({ id: 'one', participants: [participant('site-1', 'site-1/model-1')] }),
      run({ id: 'two', participants: [participant('site-1', 'site-1/model-2')] }),
      run({ id: 'three', participants: [participant('site-2', 'site-2/model-1')] }),
    ];
    expect(
      filterIqHistory(sources, filters({ participants: [['site-1']] })).map((item) => item.id),
    ).toEqual(['one', 'two']);
    expect(
      filterIqHistory(sources, filters({ participants: [['site-1', 'site-1/model-2']] })).map(
        (item) => item.id,
      ),
    ).toEqual(['two']);
    expect(
      filterIqHistory(
        sources,
        filters({ participants: [['site-1', 'site-1/model-2'], ['site-2']] }),
      ).map((item) => item.id),
    ).toEqual(['two', 'three']);
  });

  it('站点和模型必须属于同一参与者，多个筛选维度为 AND', () => {
    const source = run(
      {
        participants: [participant('site-1', 'shared-model'), participant('site-2', 'other-model')],
        entries: [entry('completed'), entry('error')],
      },
      { value: 'partial-failure', success: 1, failed: 1 },
    );
    expect(
      filterIqHistory([source], filters({ participants: [['site-1', 'other-model']] })),
    ).toEqual([]);
    expect(
      filterIqHistory(
        [source],
        filters({ participants: [['site-1', 'shared-model']], result: 'all-success' }),
      ),
    ).toEqual([]);
    expect(
      filterIqHistory(
        [source],
        filters({
          participants: [['site-1', 'shared-model']],
          result: 'partial-failure',
          startAtMs: 1_001,
        }),
      ),
    ).toEqual([]);
    expect(
      filterIqHistory(
        [source],
        filters({
          participants: [['site-2', 'other-model']],
          result: 'partial-failure',
          startAtMs: 1_000,
        }),
      ),
    ).toEqual([source]);
  });

  it('执行结果筛选使用后端分类，不根据 entries 重新推导', () => {
    const source = run(
      { entries: [entry('completed')] },
      { value: 'all-failure', success: 0, failed: 1 },
    );
    expect(filterIqHistory([source], filters({ result: 'all-failure' }))).toEqual([source]);
    expect(filterIqHistory([source], filters({ result: 'all-success' }))).toEqual([]);
  });

  it('匹配后保留整场参与者、调用和后端汇总，不裁剪为命中的参与者', () => {
    const source = run(
      {
        participants: [participant('site-1', 'model-1'), participant('site-2', 'model-2')],
        entries: [
          entry('completed'),
          { ...entry('error'), id: 'other-entry', participantIndex: 1 },
        ],
      },
      { value: 'partial-failure', success: 1, failed: 1 },
    );
    const matches = filterIqHistory([source], filters({ participants: [['site-1', 'model-1']] }));
    expect(matches[0]).toBe(source);
    expect(matches[0]!.participants).toBe(source.participants);
    expect(matches[0]!.entries).toBe(source.entries);
    expect(matches[0]!.summary.callResult).toBe(source.summary.callResult);
    expect(getIqHistoryResult(matches[0]!)).toMatchObject({
      value: 'partial-failure',
      success: 1,
      failed: 1,
    });
  });
});
