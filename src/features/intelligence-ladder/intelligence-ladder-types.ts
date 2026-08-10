export interface IntelligenceRankingEntry {
  rank: number;
  model: string;
  effort: string;
  iq: number;
}

export interface IntelligenceAutoFetchSettings {
  enabled: boolean;
  intervalMinutes: number;
  fetchOnAppStart: boolean;
  fetchOnNetworkRestore: boolean;
}

export type IntelligenceRefreshState =
  'idle' | 'running' | 'failed' | 'scraper_unavailable' | 'not_connected';

export interface IntelligenceSafeError {
  code: string;
  message: string;
}

export interface IntelligenceLadderRuntime {
  state: IntelligenceRefreshState;
  lastSuccessfulAt: string | null;
  nextScheduledAt: string | null;
}

export interface IntelligenceLadderSnapshot {
  revision: number;
  entries: IntelligenceRankingEntry[];
  fetchedAt: string | null;
  isStale: boolean;
  lastError: IntelligenceSafeError | null;
  runtime: IntelligenceLadderRuntime;
  settings: IntelligenceAutoFetchSettings;
  allowedIntervalMinutes: number[];
}

export type IntelligenceRefreshResult =
  { status: 'started' } | { status: 'already_running' } | { status: 'scraper_unavailable' };

export interface IntelligenceLadderDataSource {
  getSnapshot(): Promise<IntelligenceLadderSnapshot>;
  refresh(): Promise<IntelligenceRefreshResult>;
  updateAutoFetchSettings(
    settings: IntelligenceAutoFetchSettings,
  ): Promise<IntelligenceLadderSnapshot>;
  subscribeToChanges?(listener: () => void): Promise<() => void>;
}
