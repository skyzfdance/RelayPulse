import { useCallback, useEffect, useMemo, useState } from 'react';
import type { PropsWithChildren } from 'react';
import { MockRelayPulseDataSource } from '../data/mock-relay-pulse-data-source';
import type { MockScenario } from '../data/mock-relay-pulse-data-source';
import type { RelayPulseDataSource } from '../data/relay-pulse-data-source';
import type { AppSnapshot } from '../domain/types';
import type { ThemeMode } from '../domain/types';
import { RelayPulseContext } from './relay-pulse-context';
import type { RelayPulseContextValue } from './relay-pulse-context';

function createDefaultDataSource() {
  const search = new URLSearchParams(window.location.search);
  const requested = search.get('scenario');
  const scenarios: MockScenario[] = ['loading', 'empty', 'error', 'action-error', 'long-text'];
  const scenario = scenarios.includes(requested as MockScenario)
    ? (requested as MockScenario)
    : 'default';
  const requestedTheme = search.get('theme');
  const themes: ThemeMode[] = ['light', 'dark', 'system'];
  const selectedTheme = themes.includes(requestedTheme as ThemeMode)
    ? (requestedTheme as ThemeMode)
    : undefined;
  return new MockRelayPulseDataSource(scenario, selectedTheme);
}

const defaultDataSource = createDefaultDataSource();

export function RelayPulseProvider({
  children,
  dataSource = defaultDataSource,
}: PropsWithChildren<{ dataSource?: RelayPulseDataSource }>) {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (operation: () => Promise<AppSnapshot>) => {
    setSnapshot(await operation());
  }, []);

  useEffect(() => {
    void dataSource
      .getSnapshot()
      .then(setSnapshot)
      .catch((cause: unknown) => {
        setError(cause instanceof Error ? cause.message : 'Mock 数据加载失败');
      })
      .finally(() => setLoading(false));
  }, [dataSource]);

  const value = useMemo<RelayPulseContextValue>(
    () => ({
      snapshot,
      loading,
      error,
      selectSite(id) {
        setSnapshot((current) => (current ? { ...current, selectedSiteId: id } : current));
      },
      createSite: (input) => run(() => dataSource.createSite(input)),
      createModel: (siteId, input) => run(() => dataSource.createModel(siteId, input)),
      updateSite: (id, input) => run(() => dataSource.updateSite(id, input)),
      deleteSite: (id) => run(() => dataSource.deleteSite(id)),
      reorderSites: (ids) => run(() => dataSource.reorderSites(ids)),
      setSiteEnabled: (id, enabled) => run(() => dataSource.setSiteEnabled(id, enabled)),
      setModelEnabled: (siteId, modelId, enabled) =>
        run(() => dataSource.setModelEnabled(siteId, modelId, enabled)),
      updateSiteAdvanced: (id, input) => run(() => dataSource.updateSiteAdvanced(id, input)),
      updateSiteProbeSettings: (siteId, settings) =>
        run(() => dataSource.updateSiteProbeSettings(siteId, settings)),
      updateModelProbeSettings: (siteId, modelId, settings) =>
        run(() => dataSource.updateModelProbeSettings(siteId, modelId, settings)),
      updateModelCustomProbeSettings: (siteId, modelId, settings) =>
        run(() => dataSource.updateModelCustomProbeSettings(siteId, modelId, settings)),
      async updateGeneralSettings(settings) {
        const previous = snapshot;
        setSnapshot((current) => (current ? { ...current, generalSettings: settings } : current));
        try {
          setSnapshot(await dataSource.updateGeneralSettings(settings));
        } catch (cause) {
          setSnapshot(previous);
          throw cause;
        }
      },
      refreshModels: (siteId) => run(() => dataSource.refreshModels(siteId)),
      refreshProbeRecords: () => run(() => dataSource.refreshProbeRecords()),
      clearProbeRecords: () => run(() => dataSource.clearProbeRecords()),
      reset: () => run(() => dataSource.reset()),
    }),
    [dataSource, error, loading, run, snapshot],
  );

  return <RelayPulseContext.Provider value={value}>{children}</RelayPulseContext.Provider>;
}
