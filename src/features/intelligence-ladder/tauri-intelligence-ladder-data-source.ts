import { invoke } from '@tauri-apps/api/core';
import type { InvokeArgs } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { backendError } from '../../data/tauri-relay-pulse-data-source';
import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
  IntelligenceRefreshResult,
} from './intelligence-ladder-types';

type InvokeIntelligenceLadderCommand = <T>(command: string, args?: InvokeArgs) => Promise<T>;
type IntelligenceLadderCommandArgs = Record<string, unknown>;
type StateChangedEvent = { revision: number; domains: string[] };
type SubscribeToChanges = (listener: (event: StateChangedEvent) => void) => Promise<() => void>;

const subscribeToTauriChanges: SubscribeToChanges = (listener) =>
  listen<StateChangedEvent>('relaypulse://state-changed', (event) => listener(event.payload));

export class TauriIntelligenceLadderDataSource implements IntelligenceLadderDataSource {
  constructor(
    private readonly invokeCommand: InvokeIntelligenceLadderCommand = invoke,
    private readonly subscribeCommand: SubscribeToChanges = subscribeToTauriChanges,
    monitorNetworkRestore = true,
  ) {
    if (monitorNetworkRestore) {
      window.addEventListener('online', () => {
        void this.action<void>('notify_intelligence_network_restored').catch(() => undefined);
      });
    }
  }

  getSnapshot(): Promise<IntelligenceLadderSnapshot> {
    return this.action('get_intelligence_ladder_snapshot');
  }

  refresh(): Promise<IntelligenceRefreshResult> {
    return this.action('refresh_intelligence_ladder');
  }

  updateAutoFetchSettings(
    settings: IntelligenceAutoFetchSettings,
  ): Promise<IntelligenceLadderSnapshot> {
    return this.action('update_intelligence_auto_fetch_settings', { settings });
  }

  subscribeToChanges(listener: () => void): Promise<() => void> {
    return this.subscribeCommand((event) => {
      if (event.domains.includes('intelligence-ladder')) listener();
    });
  }

  private async action<T>(command: string, args?: IntelligenceLadderCommandArgs): Promise<T> {
    try {
      return await this.invokeCommand<T>(command, args);
    } catch (cause) {
      throw backendError(cause);
    }
  }
}
