import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
} from './intelligence-ladder-types';

const unavailableSnapshot: IntelligenceLadderSnapshot = {
  revision: 0,
  entries: [],
  fetchedAt: null,
  isStale: false,
  lastError: null,
  runtime: {
    state: 'not_connected',
    lastSuccessfulAt: null,
    nextScheduledAt: null,
  },
  settings: {
    enabled: false,
    intervalMinutes: 10,
    fetchOnAppStart: false,
    fetchOnNetworkRestore: false,
  },
  allowedIntervalMinutes: [10, 30, 60],
};

export class UnavailableIntelligenceLadderDataSource implements IntelligenceLadderDataSource {
  async getSnapshot() {
    return {
      ...unavailableSnapshot,
      entries: [],
      runtime: { ...unavailableSnapshot.runtime },
      settings: { ...unavailableSnapshot.settings },
      allowedIntervalMinutes: [...unavailableSnapshot.allowedIntervalMinutes],
    };
  }

  async refresh() {
    return { status: 'scraper_unavailable' as const };
  }

  async updateAutoFetchSettings(
    settings: IntelligenceAutoFetchSettings,
  ): Promise<IntelligenceLadderSnapshot> {
    void settings;
    throw new Error('智力天梯数据源尚未接入');
  }
}
