import { describe, expect, it, vi } from 'vitest';
import { createFixtureSnapshot } from './fixtures';
import { TauriRelayPulseDataSource } from './tauri-relay-pulse-data-source';

function authoritativeSnapshot() {
  const { revision, sites, generalSettings, probeRecords } = createFixtureSnapshot();
  return {
    revision: revision + 4,
    sites: sites.slice(0, 1),
    generalSettings,
    probeRecords: probeRecords.slice(0, 1),
  };
}

describe('TauriRelayPulseDataSource', () => {
  it('maps authoritative sites and probe records from the backend snapshot', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    const snapshot = await dataSource.getSnapshot();

    expect(invoke).toHaveBeenCalledWith('get_snapshot', undefined);
    expect(snapshot.revision).toBe(4);
    expect(snapshot.selectedSiteId).toBe(snapshot.sites[0].id);
    expect(snapshot.probeRecords).toHaveLength(1);
    expect(snapshot.tray.healthy).toMatchObject({
      activeSites: 1,
      activeModels: 3,
      alertCount: 1,
      completedLabel: '最近完成 14:10:42',
      nextProbeLabel: '等待首次调度',
    });
  });

  it('uses authoritative model status for tray alerts and orders by site priority', async () => {
    const { revision, sites, generalSettings } = createFixtureSnapshot();
    const warningSites = sites.map((site) => ({
      ...site,
      models: site.models.map((model) => ({
        ...model,
        status: model.enabled ? ('warning' as const) : model.status,
      })),
    }));
    const invoke = vi.fn().mockResolvedValue({
      revision,
      sites: warningSites,
      generalSettings,
      probeRecords: [],
    });
    const dataSource = new TauriRelayPulseDataSource(invoke);

    const tray = (await dataSource.getSnapshot()).tray.alerts;
    const priorities = tray.alerts.map(
      (alert) => warningSites.find((site) => site.name === alert.siteName)?.priority,
    );

    expect(tray.status).toBe('warning');
    expect(priorities).toEqual([...priorities].sort((left, right) => (left ?? 0) - (right ?? 0)));
  });

  it('uses camelCase command arguments and explicit credential intent', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await dataSource.updateSite('site-a', {
      name: 'Site A',
      protocol: 'openai-compatible',
      baseUrl: 'https://example.test/v1',
      credential: { action: 'replace', secret: 'fixture-secret' },
    });

    expect(invoke).toHaveBeenCalledWith('update_site', {
      id: 'site-a',
      input: expect.objectContaining({
        credential: { action: 'replace', secret: 'fixture-secret' },
      }),
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('forwards revision events without interpreting their domain payload', async () => {
    const unsubscribe = vi.fn();
    const subscribe = vi.fn(
      async (listener: (event: { revision: number; domains: string[] }) => void) => {
        listener({ revision: 7, domains: ['sites'] });
        return unsubscribe;
      },
    );
    const dataSource = new TauriRelayPulseDataSource(vi.fn(), subscribe);
    const listener = vi.fn();

    const stop = await dataSource.subscribeToChanges(listener);

    expect(listener).toHaveBeenCalledWith({ revision: 7, domains: ['sites'] });
    stop();
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it('preserves stable structured backend failures', async () => {
    const dataSource = new TauriRelayPulseDataSource(
      vi.fn().mockRejectedValue({
        code: 'credential_unavailable',
        category: 'credential',
        message: 'The local credential file is unavailable.',
        field: null,
        retryable: true,
        operationId: null,
      }),
    );

    await expect(dataSource.getSnapshot()).rejects.toMatchObject({
      code: 'credential_unavailable',
      category: 'credential',
      retryable: true,
      operationId: null,
    });
  });

  it('maps global, site and single-model probes to their production commands', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await dataSource.runImmediateProbe();
    await dataSource.runSiteProbe('site-a');
    await dataSource.runModelProbe('site-a', 'model-disabled');

    expect(invoke).toHaveBeenCalledWith('run_immediate_probe', {
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenCalledWith('run_site_probe', {
      siteId: 'site-a',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenCalledWith('run_model_probe', {
      siteId: 'site-a',
      modelId: 'model-disabled',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('maps model refresh to the real backend command with its site id', async () => {
    const invoke = vi.fn().mockResolvedValue({
      status: 'applied',
      snapshot: authoritativeSnapshot(),
    });
    const dataSource = new TauriRelayPulseDataSource(invoke);

    const result = await dataSource.refreshModels('site-a');

    expect(result.status).toBe('applied');
    if (result.status === 'applied') expect(result.snapshot.sites).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith('refresh_models', {
      siteId: 'site-a',
      confirmedRemovedModelIds: [],
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('returns model removals without importing a new snapshot', async () => {
    const invoke = vi.fn().mockResolvedValue({
      status: 'confirmationRequired',
      removedModels: [{ id: 'model-a', name: 'Model A' }],
    });
    const dataSource = new TauriRelayPulseDataSource(invoke);

    const result = await dataSource.refreshModels('site-a', ['model-a']);

    expect(result).toEqual({
      status: 'confirmationRequired',
      removedModels: [{ id: 'model-a', name: 'Model A' }],
    });
    expect(invoke).toHaveBeenCalledWith('refresh_models', {
      siteId: 'site-a',
      confirmedRemovedModelIds: ['model-a'],
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('maps model deletion to the scoped backend command', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await dataSource.deleteModel('site-a', 'model-a');

    expect(invoke).toHaveBeenCalledWith('delete_model', {
      siteId: 'site-a',
      modelId: 'model-a',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });

  it('maps desktop diagnostics and window actions to dedicated commands', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce({
        totalBytes: 2048,
        fileCount: 1,
        maxTotalBytes: 10 * 1024 * 1024,
        configuredMaxTotalBytes: 10 * 1024 * 1024,
      })
      .mockResolvedValueOnce({
        totalBytes: 2048,
        fileCount: 1,
        maxTotalBytes: 10 * 1024 * 1024,
        configuredMaxTotalBytes: 36 * 1024 * 1024,
      })
      .mockResolvedValueOnce({
        deletedFiles: 1,
        reclaimedBytes: 2048,
        statistics: {
          totalBytes: 192,
          fileCount: 1,
          maxTotalBytes: 10 * 1024 * 1024,
          configuredMaxTotalBytes: 36 * 1024 * 1024,
        },
      })
      .mockResolvedValueOnce({ status: 'exported', path: '/tmp/diagnostics.json.gz' })
      .mockResolvedValueOnce(undefined);
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await dataSource.openLogDirectory();
    await expect(dataSource.getLogStatistics()).resolves.toEqual({
      totalBytes: 2048,
      fileCount: 1,
      maxTotalBytes: 10 * 1024 * 1024,
      configuredMaxTotalBytes: 10 * 1024 * 1024,
    });
    await expect(dataSource.setDiagnosticLogLimit(36)).resolves.toMatchObject({
      maxTotalBytes: 10 * 1024 * 1024,
      configuredMaxTotalBytes: 36 * 1024 * 1024,
    });
    await expect(dataSource.clearDiagnosticLogs()).resolves.toMatchObject({
      deletedFiles: 1,
      reclaimedBytes: 2048,
      statistics: { totalBytes: 192 },
    });
    await expect(dataSource.exportDiagnostics()).resolves.toEqual({
      status: 'exported',
      path: '/tmp/diagnostics.json.gz',
    });
    await dataSource.openMainWindow('records');

    expect(invoke).toHaveBeenNthCalledWith(1, 'open_log_directory', {
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'get_log_statistics', undefined);
    expect(invoke).toHaveBeenNthCalledWith(3, 'set_diagnostic_log_limit', {
      maxTotalMegabytes: 36,
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(4, 'clear_diagnostic_logs', {
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(5, 'export_diagnostics', {
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
    expect(invoke).toHaveBeenNthCalledWith(6, 'open_main_window', {
      view: 'records',
      operationId: expect.stringMatching(/^op_[a-zA-Z0-9_-]+$/),
    });
  });
});
