import { createFixtureSnapshot } from './fixtures';
import type { RelayPulseDataSource } from './relay-pulse-data-source';
import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ProbeSettings,
  SiteAdvancedInput,
  ThemeMode,
  UpdateSiteInput,
} from '../domain/types';

const clone = <T>(value: T): T => structuredClone(value);
export type MockScenario = 'default' | 'loading' | 'empty' | 'error' | 'long-text';

export class MockRelayPulseDataSource implements RelayPulseDataSource {
  private snapshot = createFixtureSnapshot();

  constructor(
    private readonly scenario: MockScenario = 'default',
    theme?: ThemeMode,
  ) {
    if (theme) this.snapshot.generalSettings.theme = theme;
    if (scenario === 'empty') {
      this.snapshot.sites = [];
      this.snapshot.selectedSiteId = null;
      this.snapshot.probeRecords = [];
    }
    if (scenario === 'long-text') {
      const site = this.snapshot.sites[0];
      site.name = 'OpenAI 企业级多区域故障转移与超长站点名称验证线路';
      site.models[0].name = 'gpt-enterprise-preview-with-an-extremely-long-model-identifier';
      site.models[0].firstToken = '128888 ms';
      site.models[0].totalTime = '9999.99 s';
      this.snapshot.probeRecords[0].siteName = site.name;
      this.snapshot.probeRecords[0].modelName = site.models[0].name;
      this.snapshot.probeRecords[0].result =
        '响应正常，但这是用于验证表格超长错误摘要不会挤压其他列的固定文本';
    }
  }

  private result(): Promise<AppSnapshot> {
    if (this.scenario === 'loading') return new Promise(() => undefined);
    if (this.scenario === 'error') return Promise.reject(new Error('Mock 数据加载失败，请重试。'));
    return Promise.resolve(clone(this.snapshot));
  }

  getSnapshot(): Promise<AppSnapshot> {
    return this.result();
  }

  async getSite(id: string) {
    return clone(this.snapshot.sites.find((site) => site.id === id) ?? null);
  }

  createSite(input: CreateSiteInput): Promise<AppSnapshot> {
    const nextIndex = this.snapshot.sites.length + 1;
    const id = `mock-site-${nextIndex}`;
    this.snapshot.sites.push({
      id,
      name: input.name,
      protocol: input.protocol,
      baseUrl: input.baseUrl,
      apiKeyMasked: input.apiKey ? '••••••••••••••••' : '未设置',
      enabled: input.enabled,
      priority: nextIndex,
      status: 'muted',
      summary: '0 个模型启用 · 不参与探测',
      modelsEndpoint: '/models',
      probeEndpoint: '/chat/completions',
      firstTokenTimeoutSeconds: 8,
      totalTimeoutSeconds: 30,
      headers: [],
      defaultProbeSettings: {
        mode: 'disabled-thinking',
        maxOutputTokens: 8,
        prompt: '只回复 OK',
        parameters: [],
      },
      models: [],
    });
    this.snapshot.selectedSiteId = id;
    return this.result();
  }

  createModel(siteId: string, input: CreateModelInput): Promise<AppSnapshot> {
    const site = this.requireSite(siteId);
    let suffix = site.models.length + 1;
    while (site.models.some((model) => model.id === `${siteId}-manual-${suffix}`)) suffix += 1;

    site.models.push({
      id: `${siteId}-manual-${suffix}`,
      name: input.name.trim(),
      enabled: input.enabled,
      status: 'muted',
      inferenceSummary: '继承站点默认',
      firstToken: '—',
      totalTime: '—',
      capabilities: ['待首次探测识别'],
      probeSettings: {
        ...clone(site.defaultProbeSettings),
        mode: 'inherit',
      },
      customProbe: {
        enabled: false,
        endpoint: '',
        parameters: [],
      },
    });
    this.updateParticipationSummary(site);
    return this.result();
  }

  updateSite(id: string, input: UpdateSiteInput): Promise<AppSnapshot> {
    const site = this.requireSite(id);
    Object.assign(site, input);
    return this.result();
  }

  deleteSite(id: string): Promise<AppSnapshot> {
    this.snapshot.sites = this.snapshot.sites.filter((site) => site.id !== id);
    this.snapshot.probeRecords = this.snapshot.probeRecords.filter(
      (record) => record.siteId !== id,
    );
    this.snapshot.sites.forEach((site, index) => {
      site.priority = index + 1;
    });
    this.snapshot.selectedSiteId = this.snapshot.sites[0]?.id ?? null;
    return this.result();
  }

  reorderSites(siteIds: string[]): Promise<AppSnapshot> {
    const byId = new Map(this.snapshot.sites.map((site) => [site.id, site]));
    this.snapshot.sites = siteIds.flatMap((id) => {
      const site = byId.get(id);
      return site ? [site] : [];
    });
    this.snapshot.sites.forEach((site, index) => {
      site.priority = index + 1;
    });
    return this.result();
  }

  setSiteEnabled(id: string, enabled: boolean): Promise<AppSnapshot> {
    const site = this.requireSite(id);
    site.enabled = enabled;
    this.updateParticipationSummary(site);
    return this.result();
  }

  setModelEnabled(siteId: string, modelId: string, enabled: boolean): Promise<AppSnapshot> {
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.enabled = enabled;
    this.updateParticipationSummary(site);
    return this.result();
  }

  updateSiteAdvanced(id: string, input: SiteAdvancedInput): Promise<AppSnapshot> {
    Object.assign(this.requireSite(id), input);
    return this.result();
  }

  updateSiteProbeSettings(siteId: string, settings: ProbeSettings): Promise<AppSnapshot> {
    this.requireSite(siteId).defaultProbeSettings = clone(settings);
    return this.result();
  }

  updateModelProbeSettings(
    siteId: string,
    modelId: string,
    settings: ProbeSettings,
  ): Promise<AppSnapshot> {
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.probeSettings = clone(settings);
    target.inferenceSummary =
      settings.mode === 'disabled-thinking'
        ? '自动 · 关闭思考'
        : settings.mode === 'minimum-reasoning'
          ? '自动 · 最低推理'
          : '继承站点默认';
    return this.result();
  }

  updateModelCustomProbeSettings(
    siteId: string,
    modelId: string,
    settings: CustomProbeSettings,
  ): Promise<AppSnapshot> {
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.customProbe = clone(settings);
    target.inferenceSummary = settings.enabled
      ? '自定义探针'
      : target.probeSettings.mode === 'disabled-thinking'
        ? '自动 · 关闭思考'
        : target.probeSettings.mode === 'minimum-reasoning'
          ? '自动 · 最低推理'
          : '继承站点默认';
    if (settings.enabled) {
      target.firstToken = '—';
      target.totalTime = '—';
    }
    return this.result();
  }

  updateGeneralSettings(settings: GeneralSettings): Promise<AppSnapshot> {
    this.snapshot.generalSettings = clone(settings);
    return this.result();
  }

  refreshModels(siteId: string): Promise<AppSnapshot> {
    this.requireSite(siteId);
    return this.result();
  }

  refreshProbeRecords(): Promise<AppSnapshot> {
    return this.result();
  }

  clearProbeRecords(): Promise<AppSnapshot> {
    this.snapshot.probeRecords = [];
    return this.result();
  }

  async getTraySnapshot() {
    return clone(this.snapshot.tray);
  }

  reset(): Promise<AppSnapshot> {
    this.snapshot = createFixtureSnapshot();
    return this.result();
  }

  private requireSite(id: string) {
    const site = this.snapshot.sites.find((item) => item.id === id);
    if (!site) throw new Error(`Unknown site: ${id}`);
    return site;
  }

  private updateParticipationSummary(site: AppSnapshot['sites'][number]) {
    if (!site.enabled) {
      site.status = 'muted';
      site.summary = '站点已停用';
      return;
    }

    const enabledModels = site.models.filter((model) => model.enabled);
    if (enabledModels.length === 0) {
      site.status = 'muted';
      site.summary = '0 个模型启用 · 不参与探测';
      return;
    }

    const errorCount = enabledModels.filter((model) => model.status === 'error').length;
    if (errorCount > 0) {
      site.status = 'error';
      site.summary = `${site.models.length} 个模型 · ${errorCount} 个异常`;
      return;
    }

    if (enabledModels.some((model) => model.status === 'warning')) {
      site.status = 'warning';
      site.summary = `${site.models.length} 个模型 · 响应偏慢`;
      return;
    }

    if (enabledModels.some((model) => model.status === 'muted')) {
      site.status = 'muted';
      site.summary = `${site.models.length} 个模型 · 等待首次探测`;
      return;
    }

    site.status = 'healthy';
    site.summary = `${site.models.length} 个模型 · 全部正常`;
  }
}
