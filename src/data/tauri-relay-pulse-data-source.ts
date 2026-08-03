import { invoke } from '@tauri-apps/api/core';
import type { InvokeArgs } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { RelayPulseDataSource } from './relay-pulse-data-source';
import type { StateChangedEvent } from './relay-pulse-data-source';
import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ModelSyncRemoval,
  ModelSyncResult,
  ProbeSettings,
  Site,
  SiteAdvancedInput,
  TraySnapshot,
  UpdateSiteInput,
} from '../domain/types';

type AuthoritativeSnapshot = Pick<AppSnapshot, 'revision' | 'sites' | 'generalSettings'>;
type AuthoritativeModelSyncResult =
  | { status: 'applied'; snapshot: AuthoritativeSnapshot }
  | { status: 'confirmationRequired'; removedModels: ModelSyncRemoval[] };
type InvokeCommand = <T>(command: string, args?: InvokeArgs) => Promise<T>;
type SubscribeToChanges = (listener: (event: StateChangedEvent) => void) => Promise<() => void>;

const subscribeToTauriChanges: SubscribeToChanges = (listener) =>
  listen<StateChangedEvent>('relaypulse://state-changed', (event) => listener(event.payload));

interface BackendErrorPayload {
  code: string;
  category: string;
  message: string;
  field?: string | null;
  retryable: boolean;
  operationId?: string | null;
}

export class RelayPulseBackendError extends Error {
  readonly code: string;
  readonly category: string;
  readonly field: string | null;
  readonly retryable: boolean;
  readonly operationId: string | null;

  constructor(payload: BackendErrorPayload) {
    super(payload.message);
    this.name = 'RelayPulseBackendError';
    this.code = payload.code;
    this.category = payload.category;
    this.field = payload.field ?? null;
    this.retryable = payload.retryable;
    this.operationId = payload.operationId ?? null;
  }
}

export class TauriRelayPulseDataSource implements RelayPulseDataSource {
  constructor(
    private readonly invokeCommand: InvokeCommand = invoke,
    private readonly subscribeCommand: SubscribeToChanges = subscribeToTauriChanges,
  ) {}

  subscribeToChanges(listener: (event: StateChangedEvent) => void): Promise<() => void> {
    return this.subscribeCommand(listener);
  }

  async getSnapshot(): Promise<AppSnapshot> {
    return this.call('get_snapshot');
  }

  async getSite(id: string): Promise<Site | null> {
    try {
      return await this.invokeCommand<Site | null>('get_site', { id });
    } catch (cause) {
      throw backendError(cause);
    }
  }

  createSite(input: CreateSiteInput): Promise<AppSnapshot> {
    return this.call('create_site', { input });
  }

  createModel(siteId: string, input: CreateModelInput): Promise<AppSnapshot> {
    return this.call('create_model', { siteId, input });
  }

  deleteModel(siteId: string, modelId: string): Promise<AppSnapshot> {
    return this.call('delete_model', { siteId, modelId });
  }

  updateSite(id: string, input: UpdateSiteInput): Promise<AppSnapshot> {
    return this.call('update_site', { id, input });
  }

  deleteSite(id: string): Promise<AppSnapshot> {
    return this.call('delete_site', { id });
  }

  reorderSites(siteIds: string[]): Promise<AppSnapshot> {
    return this.call('reorder_sites', { siteIds });
  }

  setSiteEnabled(id: string, enabled: boolean): Promise<AppSnapshot> {
    return this.call('set_site_enabled', { id, enabled });
  }

  setModelEnabled(siteId: string, modelId: string, enabled: boolean): Promise<AppSnapshot> {
    return this.call('set_model_enabled', { siteId, modelId, enabled });
  }

  updateSiteAdvanced(id: string, input: SiteAdvancedInput): Promise<AppSnapshot> {
    return this.call('update_site_advanced', { id, input });
  }

  updateSiteProbeSettings(siteId: string, settings: ProbeSettings): Promise<AppSnapshot> {
    return this.call('update_site_probe_settings', { siteId, settings });
  }

  updateModelProbeSettings(
    siteId: string,
    modelId: string,
    settings: ProbeSettings,
  ): Promise<AppSnapshot> {
    return this.call('update_model_probe_settings', { siteId, modelId, settings });
  }

  updateModelCustomProbeSettings(
    siteId: string,
    modelId: string,
    settings: CustomProbeSettings,
  ): Promise<AppSnapshot> {
    return this.call('update_model_custom_probe_settings', { siteId, modelId, settings });
  }

  updateGeneralSettings(settings: GeneralSettings): Promise<AppSnapshot> {
    return this.call('update_general_settings', { settings });
  }

  runImmediateProbe(): Promise<AppSnapshot> {
    return this.call('run_immediate_probe');
  }

  async refreshModels(
    siteId: string,
    confirmedRemovedModelIds: string[] = [],
  ): Promise<ModelSyncResult> {
    try {
      const result = await this.invokeCommand<AuthoritativeModelSyncResult>('refresh_models', {
        siteId,
        confirmedRemovedModelIds,
      });
      return result.status === 'applied'
        ? { status: 'applied', snapshot: viewSnapshot(result.snapshot) }
        : result;
    } catch (cause) {
      throw backendError(cause);
    }
  }

  refreshProbeRecords(): Promise<AppSnapshot> {
    return this.call('refresh_probe_records');
  }

  clearProbeRecords(): Promise<AppSnapshot> {
    return this.call('clear_probe_records');
  }

  async getTraySnapshot(): Promise<{ healthy: TraySnapshot; alerts: TraySnapshot }> {
    return (await this.getSnapshot()).tray;
  }

  private async call(command: string, args?: InvokeArgs): Promise<AppSnapshot> {
    try {
      const snapshot = await this.invokeCommand<AuthoritativeSnapshot>(command, args);
      return viewSnapshot(snapshot);
    } catch (cause) {
      throw backendError(cause);
    }
  }
}

function viewSnapshot(snapshot: AuthoritativeSnapshot): AppSnapshot {
  const activeSites = snapshot.sites.filter(
    (site) => site.enabled && site.models.some((model) => model.enabled),
  ).length;
  const activeModels = snapshot.sites.reduce(
    (count, site) =>
      count + (site.enabled ? site.models.filter((model) => model.enabled).length : 0),
    0,
  );
  const tray: TraySnapshot = {
    status: 'healthy',
    activeSites,
    activeModels,
    alertCount: 0,
    alerts: [],
    completedLabel: '尚未执行探测',
    nextProbeLabel: '等待调度器启动',
  };
  return {
    ...snapshot,
    selectedSiteId: snapshot.sites[0]?.id ?? null,
    probeRecords: [],
    tray: { healthy: tray, alerts: tray },
  };
}

function backendError(cause: unknown): Error {
  if (cause instanceof RelayPulseBackendError) return cause;
  if (isBackendErrorPayload(cause)) return new RelayPulseBackendError(cause);
  return new RelayPulseBackendError({
    code: 'internal',
    category: 'internal',
    message: 'RelayPulse could not complete the operation.',
    retryable: false,
  });
}

function isBackendErrorPayload(value: unknown): value is BackendErrorPayload {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.code === 'string' &&
    typeof candidate.category === 'string' &&
    typeof candidate.message === 'string' &&
    typeof candidate.retryable === 'boolean'
  );
}
