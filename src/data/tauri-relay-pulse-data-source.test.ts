import { describe, expect, it, vi } from 'vitest';
import { createFixtureSnapshot } from './fixtures';
import { TauriRelayPulseDataSource } from './tauri-relay-pulse-data-source';

function authoritativeSnapshot() {
  const { revision, sites, generalSettings } = createFixtureSnapshot();
  return { revision: revision + 4, sites: sites.slice(0, 1), generalSettings };
}

describe('TauriRelayPulseDataSource', () => {
  it('maps the authoritative snapshot without importing probe fixtures', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    const snapshot = await dataSource.getSnapshot();

    expect(invoke).toHaveBeenCalledWith('get_snapshot', undefined);
    expect(snapshot.revision).toBe(4);
    expect(snapshot.selectedSiteId).toBe(snapshot.sites[0].id);
    expect(snapshot.probeRecords).toEqual([]);
    expect(snapshot.tray.healthy).toMatchObject({
      activeSites: 1,
      activeModels: 3,
      alertCount: 0,
      completedLabel: '尚未执行探测',
    });
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

  it('maps immediate probe to its production command instead of the mock reset', async () => {
    const invoke = vi.fn().mockRejectedValue({
      code: 'not_implemented',
      category: 'internal',
      message: 'This RelayPulse operation is not available yet.',
      retryable: false,
    });
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await expect(dataSource.runImmediateProbe()).rejects.toMatchObject({
      code: 'not_implemented',
    });
    expect(invoke).toHaveBeenCalledWith('run_immediate_probe', undefined);
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
    });
  });

  it('maps model deletion to the scoped backend command', async () => {
    const invoke = vi.fn().mockResolvedValue(authoritativeSnapshot());
    const dataSource = new TauriRelayPulseDataSource(invoke);

    await dataSource.deleteModel('site-a', 'model-a');

    expect(invoke).toHaveBeenCalledWith('delete_model', {
      siteId: 'site-a',
      modelId: 'model-a',
    });
  });
});
