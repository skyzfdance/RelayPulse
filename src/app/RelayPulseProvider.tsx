import { useCallback, useEffect, useMemo, useState } from 'react';
import type { PropsWithChildren } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { MockRelayPulseDataSource } from '../data/mock-relay-pulse-data-source';
import type { MockScenario } from '../data/mock-relay-pulse-data-source';
import type { RelayPulseDataSource } from '../data/relay-pulse-data-source';
import { TauriRelayPulseDataSource } from '../data/tauri-relay-pulse-data-source';
import { isNewerRevision } from '../data/revision';
import type { AppSnapshot } from '../domain/types';
import type { ThemeMode } from '../domain/types';
import { recordFrontendAction } from './frontend-diagnostics';
import {
  MockNotificationDataSource,
  notificationScenarioFromSearch,
} from '../features/notifications/mock-notification-data-source';
import type { NotificationDataSource } from '../features/notifications/notification-types';
import { TauriNotificationDataSource } from '../features/notifications/tauri-notification-data-source';
import {
  MockOperationHistoryDataSource,
  operationHistoryScenarioFromSearch,
} from '../features/operation-history/mock-operation-history-data-source';
import type { OperationHistoryDataSource } from '../features/operation-history/operation-history-types';
import { TauriOperationHistoryDataSource } from '../features/operation-history/tauri-operation-history-data-source';
import { RelayPulseContext } from './relay-pulse-context';
import type { RelayPulseContextValue } from './relay-pulse-context';

function createDefaultDataSource() {
  if (isTauri()) return new TauriRelayPulseDataSource();

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

function createDefaultNotificationDataSource(): NotificationDataSource {
  if (isTauri()) return new TauriNotificationDataSource();
  return new MockNotificationDataSource(notificationScenarioFromSearch(window.location.search));
}

const defaultNotificationDataSource = createDefaultNotificationDataSource();

function createDefaultOperationHistoryDataSource(): OperationHistoryDataSource {
  if (isTauri()) return new TauriOperationHistoryDataSource();
  return new MockOperationHistoryDataSource(
    operationHistoryScenarioFromSearch(window.location.search),
  );
}

const defaultOperationHistoryDataSource = createDefaultOperationHistoryDataSource();

function preserveSelection(current: AppSnapshot | null, next: AppSnapshot): AppSnapshot {
  const selectedSiteId = current?.selectedSiteId;
  return {
    ...next,
    selectedSiteId: next.sites.some((site) => site.id === selectedSiteId)
      ? (selectedSiteId ?? null)
      : next.selectedSiteId,
  };
}

export function RelayPulseProvider({
  children,
  dataSource = defaultDataSource,
  notificationDataSource = defaultNotificationDataSource,
  operationHistoryDataSource = defaultOperationHistoryDataSource,
}: PropsWithChildren<{
  dataSource?: RelayPulseDataSource;
  notificationDataSource?: NotificationDataSource;
  operationHistoryDataSource?: OperationHistoryDataSource;
}>) {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (operation: () => Promise<AppSnapshot>) => {
    const next = await operation();
    setSnapshot((current) => preserveSelection(current, next));
  }, []);

  useEffect(() => {
    void dataSource
      .getSnapshot()
      .then(setSnapshot)
      .catch((cause: unknown) => {
        setError(cause instanceof Error ? cause.message : '后端数据加载失败');
      })
      .finally(() => setLoading(false));
  }, [dataSource]);

  useEffect(() => {
    if (!dataSource.subscribeToChanges) return;
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    const currentRevision = snapshot?.revision ?? -1;
    void dataSource
      .subscribeToChanges((event) => {
        if (!isNewerRevision(currentRevision < 0 ? null : currentRevision, event.revision)) return;
        void dataSource
          .getSnapshot()
          .then((next) => {
            if (!disposed) setSnapshot((current) => preserveSelection(current, next));
          })
          .catch((cause: unknown) => {
            if (!disposed) {
              setError(cause instanceof Error ? cause.message : '后端数据重新同步失败');
            }
          });
      })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [dataSource, snapshot?.revision]);

  const value = useMemo<RelayPulseContextValue>(
    () => ({
      notificationDataSource,
      operationHistoryDataSource,
      snapshot,
      loading,
      error,
      selectSite(id) {
        recordFrontendAction('select-site', { targetType: 'site', targetId: id });
        setSnapshot((current) => (current ? { ...current, selectedSiteId: id } : current));
      },
      createSite: (input) => run(() => dataSource.createSite(input)),
      createModel: (siteId, input) => run(() => dataSource.createModel(siteId, input)),
      deleteModel: (siteId, modelId) => run(() => dataSource.deleteModel(siteId, modelId)),
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
          const next = await dataSource.updateGeneralSettings(settings);
          setSnapshot((current) => preserveSelection(current, next));
        } catch (cause) {
          setSnapshot(previous);
          throw cause;
        }
      },
      runImmediateProbe: () => run(() => dataSource.runImmediateProbe()),
      runSiteProbe: (siteId) => run(() => dataSource.runSiteProbe(siteId)),
      runModelProbe: (siteId, modelId) => run(() => dataSource.runModelProbe(siteId, modelId)),
      async refreshModels(siteId, confirmedRemovedModelIds) {
        const result = await dataSource.refreshModels(siteId, confirmedRemovedModelIds);
        if (result.status === 'applied') {
          setSnapshot((current) => preserveSelection(current, result.snapshot));
        }
        return result;
      },
      refreshProbeRecords: () => run(() => dataSource.refreshProbeRecords()),
      clearProbeRecords: () => run(() => dataSource.clearProbeRecords()),
      openLogDirectory: () => dataSource.openLogDirectory(),
      getLogStatistics: () => dataSource.getLogStatistics(),
      setDiagnosticLogLimit: (maxTotalMegabytes) =>
        dataSource.setDiagnosticLogLimit(maxTotalMegabytes),
      clearDiagnosticLogs: () => dataSource.clearDiagnosticLogs(),
      exportDiagnostics: () => dataSource.exportDiagnostics(),
      openMainWindow: (view) => dataSource.openMainWindow(view),
    }),
    [dataSource, error, loading, notificationDataSource, operationHistoryDataSource, run, snapshot],
  );

  return <RelayPulseContext.Provider value={value}>{children}</RelayPulseContext.Provider>;
}
