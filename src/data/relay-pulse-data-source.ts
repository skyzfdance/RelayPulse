import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ModelSyncResult,
  ProbeSettings,
  Site,
  SiteAdvancedInput,
  TraySnapshot,
  UpdateSiteInput,
} from '../domain/types';

export interface StateChangedEvent {
  revision: number;
  domains: string[];
}

export interface RelayPulseDataSource {
  subscribeToChanges?(listener: (event: StateChangedEvent) => void): Promise<() => void>;
  getSnapshot(): Promise<AppSnapshot>;
  getSite(id: string): Promise<Site | null>;
  createSite(input: CreateSiteInput): Promise<AppSnapshot>;
  createModel(siteId: string, input: CreateModelInput): Promise<AppSnapshot>;
  deleteModel(siteId: string, modelId: string): Promise<AppSnapshot>;
  updateSite(id: string, input: UpdateSiteInput): Promise<AppSnapshot>;
  deleteSite(id: string): Promise<AppSnapshot>;
  reorderSites(siteIds: string[]): Promise<AppSnapshot>;
  setSiteEnabled(id: string, enabled: boolean): Promise<AppSnapshot>;
  setModelEnabled(siteId: string, modelId: string, enabled: boolean): Promise<AppSnapshot>;
  updateSiteAdvanced(id: string, input: SiteAdvancedInput): Promise<AppSnapshot>;
  updateSiteProbeSettings(siteId: string, settings: ProbeSettings): Promise<AppSnapshot>;
  updateModelProbeSettings(
    siteId: string,
    modelId: string,
    settings: ProbeSettings,
  ): Promise<AppSnapshot>;
  updateModelCustomProbeSettings(
    siteId: string,
    modelId: string,
    settings: CustomProbeSettings,
  ): Promise<AppSnapshot>;
  updateGeneralSettings(settings: GeneralSettings): Promise<AppSnapshot>;
  runImmediateProbe(): Promise<AppSnapshot>;
  refreshModels(siteId: string, confirmedRemovedModelIds?: string[]): Promise<ModelSyncResult>;
  refreshProbeRecords(): Promise<AppSnapshot>;
  clearProbeRecords(): Promise<AppSnapshot>;
  getTraySnapshot(): Promise<{ healthy: TraySnapshot; alerts: TraySnapshot }>;
}
