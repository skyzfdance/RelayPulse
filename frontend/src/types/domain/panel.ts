/** 小面板的展示契约，独立于站点配置和历史探测记录。 */
import type { ThemeMode } from '@/types/theme';

export type PanelState = 'waiting' | 'ready';

export type PanelModelStatus = 'healthy' | 'unavailable' | 'slow' | 'pending';

export interface PanelModelResult {
  id: string;
  modelName: string;
  status: PanelModelStatus;
  /** 缺失耗时明确使用 null，不以 0 表示。 */
  firstTokenMs: number | null;
  totalMs: number | null;
}

export interface PanelSiteGroup {
  /** 使用稳定站点 ID 标识分组，不按显示名称合并站点。 */
  siteId: string;
  siteName: string;
  models: PanelModelResult[];
}

export interface PanelProbeCompletion {
  completedAt: string;
  completedCount: number;
  totalCount: number;
}

export interface PanelProbeProgress {
  completedCount: number;
  totalCount: number;
}

export type PanelSchedule = { state: 'scheduled'; nextProbeAt: string } | { state: 'disabled' };

export interface PanelSnapshot {
  /** 当前后端进程内的单调版本，用于拒绝晚到的旧快照。 */
  revision: number;
  /** 后端已保存的主题偏好，与业务数据共用版本及失效通知。 */
  theme: ThemeMode;
  /** 只区分等待首次结果和已有结果，不根据行状态或历史记录重新推导。 */
  state: PanelState;
  siteCount: number;
  modelCount: number;
  /** 数据层按站点组织的模型摘要，展示层保持站点及组内模型的原顺序。 */
  sites: PanelSiteGroup[];
  lastProbe: PanelProbeCompletion | null;
  /** null 表示当前没有正在进行的探测。 */
  activeProbe: PanelProbeProgress | null;
  schedule: PanelSchedule;
}

/** 失效通知；收到后读取完整快照，不包含探测结果。 */
export interface PanelStateChanged {
  revision: number;
}
