import { createContext, useContext } from 'react';
import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ModelSyncResult,
  ProbeSettings,
  SiteAdvancedInput,
  UpdateSiteInput,
} from '../domain/types';
import type {
  DiagnosticExportResult,
  LogCleanupResult,
  LogStatistics,
} from '../data/relay-pulse-data-source';

export interface RelayPulseContextValue {
  snapshot: AppSnapshot | null;
  loading: boolean;
  error: string | null;
  selectSite(id: string): void;
  createSite(input: CreateSiteInput): Promise<void>;
  createModel(siteId: string, input: CreateModelInput): Promise<void>;
  deleteModel(siteId: string, modelId: string): Promise<void>;
  updateSite(id: string, input: UpdateSiteInput): Promise<void>;
  deleteSite(id: string): Promise<void>;
  reorderSites(siteIds: string[]): Promise<void>;
  setSiteEnabled(id: string, enabled: boolean): Promise<void>;
  setModelEnabled(siteId: string, modelId: string, enabled: boolean): Promise<void>;
  updateSiteAdvanced(id: string, input: SiteAdvancedInput): Promise<void>;
  updateSiteProbeSettings(siteId: string, settings: ProbeSettings): Promise<void>;
  updateModelProbeSettings(siteId: string, modelId: string, settings: ProbeSettings): Promise<void>;
  updateModelCustomProbeSettings(
    siteId: string,
    modelId: string,
    settings: CustomProbeSettings,
  ): Promise<void>;
  updateGeneralSettings(settings: GeneralSettings): Promise<void>;
  runImmediateProbe(): Promise<void>;
  runSiteProbe(siteId: string): Promise<void>;
  runModelProbe(siteId: string, modelId: string): Promise<void>;
  refreshModels(siteId: string, confirmedRemovedModelIds?: string[]): Promise<ModelSyncResult>;
  refreshProbeRecords(): Promise<void>;
  clearProbeRecords(): Promise<void>;
  openLogDirectory(): Promise<void>;
  getLogStatistics(): Promise<LogStatistics>;
  setDiagnosticLogLimit(maxTotalMegabytes: number): Promise<LogStatistics>;
  clearDiagnosticLogs(): Promise<LogCleanupResult>;
  exportDiagnostics(): Promise<DiagnosticExportResult>;
  openMainWindow(view?: 'sites' | 'general' | 'records'): Promise<void>;
}

export const RelayPulseContext = createContext<RelayPulseContextValue | null>(null);

export function useRelayPulse() {
  const value = useContext(RelayPulseContext);
  if (!value) throw new Error('useRelayPulse must be used inside RelayPulseProvider');
  return value;
}
