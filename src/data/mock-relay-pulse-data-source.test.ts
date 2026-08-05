import { beforeEach, describe, expect, it } from 'vitest';
import { MockRelayPulseDataSource } from './mock-relay-pulse-data-source';

describe('MockRelayPulseDataSource', () => {
  let dataSource: MockRelayPulseDataSource;

  beforeEach(() => {
    dataSource = new MockRelayPulseDataSource();
  });

  it('reorders sites and rewrites priorities', async () => {
    const before = await dataSource.getSnapshot();
    const ids = before.sites.map((site) => site.id).reverse();
    const after = await dataSource.reorderSites(ids);

    expect(after.sites.map((site) => site.id)).toEqual(ids);
    expect(after.sites.map((site) => site.priority)).toEqual([1, 2, 3, 4, 5]);
  });

  it('creates and deletes only the in-memory snapshot', async () => {
    const created = await dataSource.createSite({
      name: '测试站点',
      protocol: 'openai-compatible',
      baseUrl: 'https://mock.example/v1',
      credential: { action: 'replace', secret: 'fixture-only' },
      enabled: true,
    });
    const id = created.selectedSiteId;
    expect(id).toBeTruthy();
    expect(created.sites[created.sites.length - 1]?.credential).toEqual({
      configured: true,
      masked: '••••••••••••••••',
    });

    await dataSource.deleteSite(id!);
    expect((await dataSource.getSnapshot()).sites.some((site) => site.id === id)).toBe(false);

    await dataSource.reset();
    expect((await dataSource.getSnapshot()).sites).toHaveLength(5);
  });

  it('deletes a model only from its owning site', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    const model = site.models[0];

    const after = await dataSource.deleteModel(site.id, model.id);

    expect(after.sites[0].models.some((item) => item.id === model.id)).toBe(false);
    expect(after.sites.slice(1)).toEqual(before.sites.slice(1));
  });

  it('returns an applied result for mock model synchronization', async () => {
    const site = (await dataSource.getSnapshot()).sites[0];

    const result = await dataSource.refreshModels(site.id);

    expect(result.status).toBe('applied');
    if (result.status === 'applied') expect(result.snapshot.sites[0].id).toBe(site.id);
  });

  it('returns cloned values that cannot mutate the fixture', async () => {
    const first = await dataSource.getSnapshot();
    first.sites[0].name = '外部修改';
    expect((await dataSource.getSnapshot()).sites[0].name).toBe('OpenAI 主线路');
  });

  it('uses explicit keep, replace and clear credential intents', async () => {
    const site = (await dataSource.getSnapshot()).sites[0];

    const kept = await dataSource.updateSite(site.id, {
      name: site.name,
      protocol: site.protocol,
      baseUrl: site.baseUrl,
      credential: { action: 'keep' },
    });
    expect(kept.sites[0].credential).toEqual(site.credential);

    const cleared = await dataSource.updateSite(site.id, {
      name: site.name,
      protocol: site.protocol,
      baseUrl: site.baseUrl,
      credential: { action: 'clear' },
    });
    expect(cleared.sites[0].credential).toEqual({ configured: false, masked: null });

    const replaced = await dataSource.updateSite(site.id, {
      name: site.name,
      protocol: site.protocol,
      baseUrl: site.baseUrl,
      credential: { action: 'replace', secret: 'new-fixture-only' },
    });
    expect(replaced.sites[0].credential.configured).toBe(true);
  });

  it('excludes enabled sites when all of their models are disabled', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    for (const model of site.models.filter((item) => item.enabled)) {
      await dataSource.setModelEnabled(site.id, model.id, false);
    }

    const after = await dataSource.getSite(site.id);
    expect(after).toMatchObject({
      enabled: true,
      status: 'muted',
      summary: '0 个模型启用 · 不参与探测',
    });
  });

  it('provides deterministic empty, error and long-text scenarios', async () => {
    expect((await new MockRelayPulseDataSource('empty').getSnapshot()).sites).toHaveLength(0);
    await expect(new MockRelayPulseDataSource('error').getSnapshot()).rejects.toThrow(
      'Mock 数据加载失败，请重试。',
    );

    const longText = await new MockRelayPulseDataSource('long-text').getSnapshot();
    expect(longText.sites[0].name).toContain('超长站点名称');
    expect(longText.sites[0].models[0].totalTime).toBe('9999.99 s');
  });

  it('rejects backend actions without mutating the visible snapshot', async () => {
    const failingDataSource = new MockRelayPulseDataSource('action-error');
    const before = await failingDataSource.getSnapshot();

    expect(() => failingDataSource.setSiteEnabled(before.sites[0].id, false)).toThrow(
      'Mock 后端操作失败，请重试。',
    );

    const after = await failingDataSource.getSnapshot();
    expect(after.sites[0].enabled).toBe(before.sites[0].enabled);
  });

  it('supports deterministic theme previews', async () => {
    const dark = await new MockRelayPulseDataSource('default', 'dark').getSnapshot();
    const light = await new MockRelayPulseDataSource('default', 'light').getSnapshot();
    expect(dark.generalSettings.theme).toBe('dark');
    expect(light.generalSettings.theme).toBe('light');
  });

  it('provides reported token usage and clears only probe records', async () => {
    const before = await dataSource.getSnapshot();
    expect(before.probeRecords).toHaveLength(14);
    expect(
      before.probeRecords.filter((record) => record.tokenUsage.source === 'reported'),
    ).toHaveLength(11);

    const streamRecord = before.probeRecords.find((record) => record.id === 'probe-001')!;
    expect(streamRecord.exchange.request.headers).toContainEqual({
      key: 'Authorization',
      value: 'Bearer ••••••••••••••••',
    });
    expect(streamRecord.exchange.response.mode).toBe('stream');
    expect(streamRecord.exchange.response.chunks).toHaveLength(2);
    expect(streamRecord.exchange.response.rawBody).toContain('data:');
    expect(streamRecord.exchange.response.truncated).toBe(false);

    const synchronousRecord = before.probeRecords.find((record) => record.id === 'probe-004')!;
    expect(synchronousRecord.exchange.response.mode).toBe('synchronous');
    expect(synchronousRecord.exchange.response.chunks).toHaveLength(0);
    expect(synchronousRecord.exchange.response.rawBody).toContain('"choices"');

    const timeoutRecord = before.probeRecords.find((record) => record.id === 'probe-003')!;
    expect(timeoutRecord.exchange.response.headers).toHaveLength(0);
    expect(timeoutRecord.exchange.response.rawBody).toBe('');

    const refreshed = await dataSource.refreshProbeRecords();
    expect(refreshed.probeRecords).toEqual(before.probeRecords);

    const cleared = await dataSource.clearProbeRecords();
    expect(cleared.probeRecords).toHaveLength(0);
    expect(cleared.sites).toHaveLength(5);

    const reset = await dataSource.reset();
    expect(reset.probeRecords).toHaveLength(14);
  });

  it('updates site defaults without changing individual model overrides', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    const modelSettings = site.models.map((model) => model.probeSettings);

    const after = await dataSource.updateSiteProbeSettings(site.id, {
      mode: 'minimum-reasoning',
      maxOutputTokens: 4,
      prompt: 'ping',
      parameters: [],
    });

    expect(after.sites[0].defaultProbeSettings).toMatchObject({
      mode: 'minimum-reasoning',
      maxOutputTokens: 4,
      prompt: 'ping',
    });
    expect(after.sites[0].models.map((model) => model.probeSettings)).toEqual(modelSettings);
  });

  it('adds a deterministic model that inherits the site defaults', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    const after = await dataSource.createModel(site.id, {
      name: 'gpt-new-preview',
      enabled: true,
    });
    const created = after.sites[0].models[after.sites[0].models.length - 1];

    expect(created).toMatchObject({
      id: 'openai-main-manual-5',
      name: 'gpt-new-preview',
      enabled: true,
      status: 'muted',
      inferenceSummary: '继承站点默认',
      firstToken: '—',
      totalTime: '—',
      probeSettings: {
        mode: 'inherit',
        maxOutputTokens: 8,
        prompt: '只回复 OK',
      },
    });
    expect(after.sites[0].summary).toBe('5 个模型 · 响应偏慢');
  });

  it('stores a custom model endpoint without changing the shared site endpoint', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    const model = site.models[0];
    const after = await dataSource.updateModelCustomProbeSettings(site.id, model.id, {
      enabled: true,
      endpoint: '/v1/images/generations',
      parameters: [{ key: 'prompt', value: '生成一个白色方块', type: 'string' }],
    });

    expect(after.sites[0].probeEndpoint).toBe(site.probeEndpoint);
    expect(after.sites[0].models[0]).toMatchObject({
      inferenceSummary: '自定义探针',
      firstToken: '—',
      totalTime: '—',
      customProbe: {
        enabled: true,
        endpoint: '/v1/images/generations',
      },
    });
  });

  it('stores a per-site probe interval with the advanced settings', async () => {
    const before = await dataSource.getSnapshot();
    const site = before.sites[0];
    const after = await dataSource.updateSiteAdvanced(site.id, {
      modelsEndpoint: site.modelsEndpoint,
      probeEndpoint: site.probeEndpoint,
      probeIntervalMinutes: 15,
      slowFirstTokenSeconds: 3,
      slowTotalSeconds: 12,
      failureThreshold: 3,
      recoveryThreshold: 2,
      headers: site.headers,
    });

    expect(after.sites[0].probeIntervalMinutes).toBe(15);
    expect(after.sites[0]).toMatchObject({
      slowFirstTokenSeconds: 3,
      slowTotalSeconds: 12,
      failureThreshold: 3,
      recoveryThreshold: 2,
    });
  });
});
