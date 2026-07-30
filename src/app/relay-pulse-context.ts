import { createContext, useContext } from 'react';
import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ProbeSettings,
  SiteAdvancedInput,
  UpdateSiteInput,
} from '../domain/types';

export interface RelayPulseContextValue {
  snapshot: AppSnapshot | null;
  loading: boolean;
  error: string | null;
  selectSite(id: string): void;
  createSite(input: CreateSiteInput): Promise<void>;
  createModel(siteId: string, input: CreateModelInput): Promise<void>;
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
  refreshModels(siteId: string): Promise<void>;
  refreshProbeRecords(): Promise<void>;
  clearProbeRecords(): Promise<void>;
  reset(): Promise<void>;
}

export const RelayPulseContext = createContext<RelayPulseContextValue | null>(null);

export function useRelayPulse() {
  const value = useContext(RelayPulseContext);
  if (!value) throw new Error('useRelayPulse must be used inside RelayPulseProvider');
  return value;
}
