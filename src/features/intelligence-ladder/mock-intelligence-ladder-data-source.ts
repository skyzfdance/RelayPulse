import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
  IntelligenceRankingEntry,
} from './intelligence-ladder-types';

export type IntelligenceLadderMockScenario =
  | 'default'
  | 'loading'
  | 'empty'
  | 'refreshing'
  | 'stale-error'
  | 'load-error'
  | 'scraper-unavailable'
  | 'save-error'
  | 'long-text';

const mockScenarios: IntelligenceLadderMockScenario[] = [
  'default',
  'loading',
  'empty',
  'refreshing',
  'stale-error',
  'load-error',
  'scraper-unavailable',
  'save-error',
  'long-text',
];

const rankingFixture: IntelligenceRankingEntry[] = [
  { rank: 1, model: 'GPT-5.6 Sol', effort: 'XHIGH', iq: 108.5 },
  { rank: 2, model: 'GPT-5.6 Sol', effort: 'ULTRA', iq: 104.5 },
  { rank: 3, model: 'GPT-5.6 Sol', effort: 'MAX', iq: 101.8 },
  { rank: 4, model: 'GPT-5.5', effort: 'XHIGH', iq: 96.4 },
  { rank: 5, model: 'GPT-5.5', effort: 'HIGH', iq: 95.1 },
  { rank: 6, model: 'GPT-5.6 Sol', effort: 'MEDIUM', iq: 93.8 },
  { rank: 7, model: 'GPT-5.6 Luna', effort: 'MAX', iq: 93.3 },
  { rank: 8, model: 'GPT-5.6 Terra', effort: 'MAX', iq: 92.4 },
  { rank: 9, model: 'GPT-5.6 Luna', effort: 'XHIGH', iq: 87.1 },
  { rank: 10, model: 'GPT-5.6 Terra', effort: 'XHIGH', iq: 81.7 },
];

const defaultSettings: IntelligenceAutoFetchSettings = {
  enabled: true,
  intervalMinutes: 10,
  fetchOnAppStart: true,
  fetchOnNetworkRestore: true,
};

function cloneSnapshot(snapshot: IntelligenceLadderSnapshot): IntelligenceLadderSnapshot {
  return {
    ...snapshot,
    entries: snapshot.entries.map((entry) => ({ ...entry })),
    lastError: snapshot.lastError ? { ...snapshot.lastError } : null,
    runtime: { ...snapshot.runtime },
    settings: { ...snapshot.settings },
    allowedIntervalMinutes: [...snapshot.allowedIntervalMinutes],
  };
}

function createSnapshot(scenario: IntelligenceLadderMockScenario): IntelligenceLadderSnapshot {
  const hasEntries = !['empty', 'load-error'].includes(scenario);
  const entries = hasEntries ? rankingFixture.map((entry) => ({ ...entry })) : [];
  if (scenario === 'long-text' && entries[0]) {
    entries[0] = {
      ...entries[0],
      model: '<img src=x onerror=alert(1)> GPT-5.6 跨区域超长模型名称与字符安全显示验证',
      effort: 'EXTREMELY-LONG-REASONING-EFFORT-LABEL',
    };
  }

  const staleError = scenario === 'stale-error';
  const loadError = scenario === 'load-error';
  const unavailable = scenario === 'scraper-unavailable';
  const running = scenario === 'refreshing';
  return {
    revision: 4,
    entries,
    fetchedAt: hasEntries ? '2026-08-09T14:05:00+08:00' : null,
    isStale: staleError || unavailable,
    lastError: loadError
      ? { code: 'load_failed', message: '尚未取得可用排名，请稍后重试。' }
      : staleError
        ? { code: 'network', message: '最近一次获取失败，当前显示上次成功数据。' }
        : unavailable
          ? { code: 'scraper_unavailable', message: '采集服务暂不可用，当前显示上次成功数据。' }
          : null,
    runtime: {
      state: running
        ? 'running'
        : unavailable
          ? 'scraper_unavailable'
          : loadError
            ? 'failed'
            : staleError
              ? 'failed'
              : 'idle',
      lastSuccessfulAt: hasEntries ? '2026-08-09T14:05:00+08:00' : null,
      nextScheduledAt: hasEntries ? '2026-08-09T14:15:00+08:00' : null,
    },
    settings: { ...defaultSettings },
    allowedIntervalMinutes: [10, 30, 60],
  };
}

export function intelligenceLadderScenarioFromSearch(
  search: string,
): IntelligenceLadderMockScenario {
  const requested = new URLSearchParams(search).get('intelligenceScenario');
  return mockScenarios.includes(requested as IntelligenceLadderMockScenario)
    ? (requested as IntelligenceLadderMockScenario)
    : 'default';
}

export class MockIntelligenceLadderDataSource implements IntelligenceLadderDataSource {
  private snapshot: IntelligenceLadderSnapshot;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly scenario: IntelligenceLadderMockScenario = 'default') {
    this.snapshot = createSnapshot(scenario);
  }

  async getSnapshot() {
    if (this.scenario === 'loading')
      return new Promise<IntelligenceLadderSnapshot>(() => undefined);
    return cloneSnapshot(this.snapshot);
  }

  async refresh() {
    if (this.snapshot.runtime.state === 'running') {
      return { status: 'already_running' as const };
    }
    if (this.snapshot.runtime.state === 'scraper_unavailable') {
      return { status: 'scraper_unavailable' as const };
    }

    this.snapshot = {
      ...this.snapshot,
      lastError: null,
      runtime: { ...this.snapshot.runtime, state: 'running' },
    };
    this.emit();
    window.setTimeout(() => {
      this.snapshot = {
        ...createSnapshot(this.scenario === 'long-text' ? 'long-text' : 'default'),
        revision: this.snapshot.revision + 1,
        settings: { ...this.snapshot.settings },
      };
      this.emit();
    }, 420);
    return { status: 'started' as const };
  }

  async updateAutoFetchSettings(settings: IntelligenceAutoFetchSettings) {
    if (this.scenario === 'save-error') throw new Error('自动获取设置保存失败');
    this.snapshot = {
      ...this.snapshot,
      revision: this.snapshot.revision + 1,
      settings: { ...settings },
      runtime: {
        ...this.snapshot.runtime,
        nextScheduledAt: settings.enabled ? this.snapshot.runtime.nextScheduledAt : null,
      },
    };
    this.emit();
    return cloneSnapshot(this.snapshot);
  }

  async subscribeToChanges(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit() {
    for (const listener of this.listeners) listener();
  }
}
