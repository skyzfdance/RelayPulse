import type { PanelSiteGroup, PanelSnapshot } from '@/types/domain/panel';

export type PanelMockScenario = 'waiting' | 'ready';

// 包含长名称、同站点多个模型和缺失耗时；分组及组内顺序由数据层明确提供。
const siteFixtures: PanelSiteGroup[] = [
  {
    siteId: 'panel-site-claude',
    siteName: 'Claude 备用线路',
    models: [
      {
        id: 'panel-claude',
        modelName: 'claude-sonnet-4-5-20250929-thinking',
        status: 'pending',
        firstTokenMs: null,
        totalMs: 22596,
      },
    ],
  },
  {
    siteId: 'panel-site-openai',
    siteName: 'OpenAI 主线路',
    models: [
      {
        id: 'panel-gpt-mini',
        modelName: 'gpt-5.4-mini',
        status: 'healthy',
        firstTokenMs: 278,
        totalMs: 1120,
      },
      {
        id: 'panel-gpt',
        modelName: 'gpt-5.4',
        status: 'slow',
        firstTokenMs: 2340,
        totalMs: 6790,
      },
    ],
  },
  {
    siteId: 'panel-site-gemini',
    siteName: 'Gemini 亚太线路（新加坡）',
    models: [
      {
        id: 'panel-gemini',
        modelName: 'gemini-2.5-pro-preview',
        status: 'unavailable',
        firstTokenMs: null,
        totalMs: 30000,
      },
    ],
  },
];

/** 每次创建独立快照，mock 之间以及刷新前后不共享可变数据。 */
export function createPanelMock(
  scenario: PanelMockScenario,
  now = new Date(),
  periodicEnabled = false,
): PanelSnapshot {
  const waiting = scenario === 'waiting';
  return {
    revision: waiting ? 0 : 1,
    theme: 'system',
    state: scenario,
    siteCount: 4,
    modelCount: 12,
    sites: waiting
      ? []
      : siteFixtures.map((site) => ({
          ...site,
          models: site.models.map((model) => ({ ...model })),
        })),
    lastProbe: waiting
      ? null
      : {
          completedAt: new Date(now.getTime() - 60000).toISOString(),
          completedCount: 12,
          totalCount: 12,
        },
    activeProbe: null,
    schedule: periodicEnabled
      ? { state: 'scheduled', nextProbeAt: new Date(now.getTime() + 60000).toISOString() }
      : { state: 'disabled' },
  };
}
