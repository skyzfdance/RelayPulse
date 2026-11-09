import type { IqParticipant, IqRun } from '@/types/domain/iq-test';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { fetchIqHistoryFilterOptions } from '@/api/iq-test/history';
import { seedSites, state as sitesState } from '@/mocks/handlers/config/sites';
import { iqState, seedDefaultIqHistory, seedIqState } from '@/mocks/handlers/iq-test';
import { operationLogFixtures } from '@/mocks/handlers/run/operation-logs';

beforeEach(() => {
  window.history.replaceState({}, '', '/');
  seedSites();
  seedIqState();
  seedDefaultIqHistory();
});

afterEach(() => {
  seedIqState();
  seedSites();
  window.history.replaceState({}, '', '/');
});

describe('IQ 历史筛选目录查询', () => {
  it('独立返回历史站点与模型身份，保留已删除来源且不受当前来源变更影响', async () => {
    const revision = iqState.revision;
    const operationCount = operationLogFixtures.length;
    const options = await fetchIqHistoryFilterOptions();

    expect(options).toHaveLength(3);
    expect(options).toEqual(
      expect.arrayContaining([
        {
          value: 'site-1',
          label: 'OpenAI 主站',
          children: [{ value: 'site-1/model-0-0', label: 'gpt-demo' }],
        },
        {
          value: 'site-2',
          label: 'Anthropic',
          children: [{ value: 'site-2/model-1-0', label: 'claude-sonnet-demo' }],
        },
        {
          value: 'retired-site',
          label: '旧备用线路',
          children: [{ value: 'retired-site/retired-model', label: 'legacy-model' }],
        },
      ]),
    );
    expect(sitesState.sites.some((site) => site.id === 'retired-site')).toBe(false);
    expect(options.some((site) => site.value === 'site-3')).toBe(false);

    sitesState.sites[0]!.name = '当前站点已改名';
    sitesState.sites[0]!.models = [];
    expect(await fetchIqHistoryFilterOptions()).toEqual(options);
    sitesState.sites = [];
    expect(await fetchIqHistoryFilterOptions()).toEqual(options);
    expect(iqState.revision).toBe(revision);
    expect(operationLogFixtures).toHaveLength(operationCount);
  });

  it('按身份去重并排序，使用最新历史名称，排除运行中的来源', async () => {
    const template = iqState.runs[0]!;
    const participant = (input: Partial<IqParticipant>): IqParticipant => ({
      ...template.participants[0]!,
      ...input,
    });
    const run = (
      id: string,
      startedAtMs: number,
      participants: IqParticipant[],
      status: IqRun['status'] = 'completed',
    ): IqRun => ({ ...template, id, startedAtMs, participants, status });

    iqState.runs = [
      run('older', 1, [
        participant({
          siteId: 'site-z',
          siteName: 'Zulu',
          modelKey: 'site-z/model-z',
          modelName: 'Zulu',
        }),
        participant({
          siteId: 'site-a',
          siteName: 'Before rename',
          modelKey: 'site-a/model-b',
          modelName: 'Before rename',
        }),
        participant({
          siteId: 'site-a',
          siteName: 'Before rename',
          modelKey: 'site-a/model-a',
          modelName: 'Alpha',
        }),
        participant({
          siteId: 'site-c',
          siteName: 'Same',
          modelKey: 'site-c/model',
          modelName: 'Shared name',
        }),
        participant({
          siteId: 'site-b',
          siteName: 'Same',
          modelKey: 'site-b/model',
          modelName: 'Shared name',
        }),
      ]),
      run(
        'newer',
        2,
        [
          participant({
            siteId: 'site-a',
            siteName: 'Alpha',
            modelKey: 'site-a/model-b',
            modelName: 'Alpha',
          }),
        ],
        'aborted',
      ),
      run(
        'running',
        3,
        [
          participant({
            siteId: 'running-only',
            siteName: 'Running only',
            modelKey: 'running-only/model',
          }),
        ],
        'running',
      ),
    ];

    expect(await fetchIqHistoryFilterOptions()).toEqual([
      {
        value: 'site-a',
        label: 'Alpha',
        children: [
          { value: 'site-a/model-a', label: 'Alpha' },
          { value: 'site-a/model-b', label: 'Alpha' },
        ],
      },
      {
        value: 'site-b',
        label: 'Same',
        children: [{ value: 'site-b/model', label: 'Shared name' }],
      },
      {
        value: 'site-c',
        label: 'Same',
        children: [{ value: 'site-c/model', label: 'Shared name' }],
      },
      {
        value: 'site-z',
        label: 'Zulu',
        children: [{ value: 'site-z/model-z', label: 'Zulu' }],
      },
    ]);
  });

  it('返回值与历史快照隔离，清空历史返回空目录，查询失败可以重试', async () => {
    const options = await fetchIqHistoryFilterOptions();
    options[0]!.label = '调用方改名';
    options[0]!.children = [];
    expect(await fetchIqHistoryFilterOptions()).not.toEqual(options);

    window.history.replaceState({}, '', '/?iqScenario=error');
    await expect(fetchIqHistoryFilterOptions()).rejects.toMatchObject({
      code: 'iq_unavailable',
      retryable: true,
    });
    window.history.replaceState({}, '', '/');
    expect(await fetchIqHistoryFilterOptions()).toHaveLength(3);

    seedIqState();
    expect(await fetchIqHistoryFilterOptions()).toEqual([]);
    seedIqState(true);
    expect(await fetchIqHistoryFilterOptions()).toEqual([]);
  });
});
