import { createFixtureSnapshot } from './fixtures';
import type { RelayPulseDataSource } from './relay-pulse-data-source';
import type {
  DiagnosticExportResult,
  LogCleanupResult,
  LogStatistics,
} from './relay-pulse-data-source';
import type {
  AppSnapshot,
  CreateModelInput,
  CreateSiteInput,
  CustomProbeSettings,
  GeneralSettings,
  ModelSyncResult,
  ProbeSettings,
  SiteAdvancedInput,
  ThemeMode,
  UpdateSiteInput,
} from '../domain/types';

const clone = <T>(value: T): T => structuredClone(value);
export type MockScenario = 'default' | 'loading' | 'empty' | 'error' | 'action-error' | 'long-text';

export class MockRelayPulseDataSource implements RelayPulseDataSource {
  private snapshot = createFixtureSnapshot();
  private logStatistics: LogStatistics = {
    totalBytes: 1_835_008,
    fileCount: 2,
    maxTotalBytes: 10 * 1024 * 1024,
    configuredMaxTotalBytes: 10 * 1024 * 1024,
  };

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

  async subscribeToChanges() {
    return () => undefined;
  }

  private result(): Promise<AppSnapshot> {
    if (this.scenario === 'loading') return new Promise(() => undefined);
    if (this.scenario === 'error') return Promise.reject(new Error('应用数据加载失败，请重试。'));
    return Promise.resolve(clone(this.snapshot));
  }

  getSnapshot(): Promise<AppSnapshot> {
    return this.result();
  }

  async getSite(id: string) {
    return clone(this.snapshot.sites.find((site) => site.id === id) ?? null);
  }

  createSite(input: CreateSiteInput): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const nextIndex = this.snapshot.sites.length + 1;
    const id = `mock-site-${nextIndex}`;
    this.snapshot.sites.push({
      id,
      name: input.name,
      protocol: input.protocol,
      baseUrl: input.baseUrl,
      credential: {
        configured: true,
        masked: '••••••••••••••••',
      },
      enabled: input.enabled,
      priority: nextIndex,
      status: 'muted',
      summary: '0 个模型启用 · 不参与探测',
      modelsEndpoint: '/models',
      probeEndpoint: '/chat/completions',
      probeIntervalMinutes: 10,
      slowFirstTokenSeconds: 2,
      slowTotalSeconds: 10,
      failureThreshold: 2,
      recoveryThreshold: 2,
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
    this.assertActionAvailable();
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

  deleteModel(siteId: string, modelId: string): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(siteId);
    if (!site.models.some((model) => model.id === modelId)) {
      throw new Error(`Unknown model: ${modelId}`);
    }
    site.models = site.models.filter((model) => model.id !== modelId);
    this.snapshot.probeRecords = this.snapshot.probeRecords.filter(
      (record) => record.siteId !== siteId || record.modelId !== modelId,
    );
    this.updateParticipationSummary(site);
    return this.result();
  }

  updateSite(id: string, input: UpdateSiteInput): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(id);
    site.name = input.name;
    site.protocol = input.protocol;
    site.baseUrl = input.baseUrl;
    if (input.credential.action === 'replace') {
      site.credential = { configured: true, masked: '••••••••••••••••' };
    } else if (input.credential.action === 'clear') {
      site.credential = { configured: false, masked: null };
    }
    return this.result();
  }

  deleteSite(id: string): Promise<AppSnapshot> {
    this.assertActionAvailable();
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
    this.assertActionAvailable();
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
    this.assertActionAvailable();
    const site = this.requireSite(id);
    site.enabled = enabled;
    this.updateParticipationSummary(site);
    return this.result();
  }

  setModelEnabled(siteId: string, modelId: string, enabled: boolean): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.enabled = enabled;
    this.updateParticipationSummary(site);
    return this.result();
  }

  updateSiteAdvanced(id: string, input: SiteAdvancedInput): Promise<AppSnapshot> {
    this.assertActionAvailable();
    Object.assign(this.requireSite(id), input);
    return this.result();
  }

  updateSiteProbeSettings(siteId: string, settings: ProbeSettings): Promise<AppSnapshot> {
    this.assertActionAvailable();
    this.requireSite(siteId).defaultProbeSettings = clone(settings);
    return this.result();
  }

  updateModelProbeSettings(
    siteId: string,
    modelId: string,
    settings: ProbeSettings,
  ): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.probeSettings = clone(settings);
    target.inferenceSummary = settings.parameters.length > 0 ? '默认 low · 透传覆盖' : '默认 · low';
    return this.result();
  }

  updateModelCustomProbeSettings(
    siteId: string,
    modelId: string,
    settings: CustomProbeSettings,
  ): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(siteId);
    const target = site.models.find((item) => item.id === modelId);
    if (!target) throw new Error(`Unknown model: ${modelId}`);
    target.customProbe = clone(settings);
    target.inferenceSummary = settings.enabled
      ? '自定义探针'
      : target.probeSettings.parameters.length > 0
        ? '默认 low · 透传覆盖'
        : '默认 · low';
    if (settings.enabled) {
      target.firstToken = '—';
      target.totalTime = '—';
    }
    return this.result();
  }

  updateGeneralSettings(settings: GeneralSettings): Promise<AppSnapshot> {
    this.assertActionAvailable();
    this.snapshot.generalSettings = clone(settings);
    return this.result();
  }

  runImmediateProbe(): Promise<AppSnapshot> {
    return this.reset();
  }

  runSiteProbe(siteId: string): Promise<AppSnapshot> {
    this.assertActionAvailable();
    this.requireSite(siteId);
    return this.result();
  }

  runModelProbe(siteId: string, modelId: string): Promise<AppSnapshot> {
    this.assertActionAvailable();
    const site = this.requireSite(siteId);
    if (!site.models.some((model) => model.id === modelId)) {
      throw new Error(`Unknown model: ${modelId}`);
    }
    return this.result();
  }

  async refreshModels(siteId: string): Promise<ModelSyncResult> {
    this.assertActionAvailable();
    this.requireSite(siteId);
    return { status: 'applied', snapshot: await this.result() };
  }

  refreshProbeRecords(): Promise<AppSnapshot> {
    this.assertActionAvailable();
    return this.result();
  }

  clearProbeRecords(): Promise<AppSnapshot> {
    this.assertActionAvailable();
    this.snapshot.probeRecords = [];
    return this.result();
  }

  async getTraySnapshot() {
    return clone(this.snapshot.tray);
  }

  async openLogDirectory(): Promise<void> {
    this.assertActionAvailable();
  }

  async getLogStatistics(): Promise<LogStatistics> {
    this.assertActionAvailable();
    return clone(this.logStatistics);
  }

  async setDiagnosticLogLimit(maxTotalMegabytes: number): Promise<LogStatistics> {
    this.assertActionAvailable();
    if (!Number.isInteger(maxTotalMegabytes) || maxTotalMegabytes < 10 || maxTotalMegabytes > 100) {
      throw new Error('日志容量上限必须是 10 到 100 之间的整数。');
    }
    this.logStatistics = {
      ...this.logStatistics,
      configuredMaxTotalBytes: maxTotalMegabytes * 1024 * 1024,
    };
    return clone(this.logStatistics);
  }

  async clearDiagnosticLogs(): Promise<LogCleanupResult> {
    this.assertActionAvailable();
    const before = this.logStatistics;
    this.logStatistics = { ...before, totalBytes: 192, fileCount: 1 };
    return {
      deletedFiles: before.fileCount,
      reclaimedBytes: before.totalBytes,
      statistics: clone(this.logStatistics),
    };
  }

  async exportDiagnostics(): Promise<DiagnosticExportResult> {
    this.assertActionAvailable();
    return { status: 'exported', path: 'RelayPulse-diagnostics-mock.json.gz' };
  }

  async openMainWindow(): Promise<void> {
    this.assertActionAvailable();
  }

  reset(): Promise<AppSnapshot> {
    this.assertActionAvailable();
    this.snapshot = createFixtureSnapshot();
    return this.result();
  }

  private requireSite(id: string) {
    const site = this.snapshot.sites.find((item) => item.id === id);
    if (!site) throw new Error(`Unknown site: ${id}`);
    return site;
  }

  private assertActionAvailable() {
    if (this.scenario === 'action-error') {
      throw new Error('Mock 后端操作失败，请重试。');
    }
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
