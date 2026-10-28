/**
 * 站点管理的具名后端方法（config/sites）。
 * 统一经 callBackend；浏览器 mock 命中 mocks/handlers/config/sites。
 * 站点契约自包含，不读写全局快照。
 */
import type {
  ApplyModelSyncInput,
  Connection,
  CreateSiteInput,
  CustomProbeRequest,
  CustomProbeTestResult,
  ManagedSite,
  ModelSyncChanges,
  ProbePolicy,
  SiteDraft,
  SiteListItem,
  SiteModel,
} from '@/types/domain/sites';

import { callBackend } from '../call-backend';

/** 读取站点列表。读操作静默，失败弹提示；加载展示由站点页面处理。 */
export function fetchSites(): Promise<SiteListItem[]> {
  return callBackend<SiteListItem[]>('get_sites', undefined, {
    successFeedback: 'none',
  });
}

/** 按站点 ID 读取详情，失败由详情区域展示并提供重试。 */
export function fetchSite(siteId: string): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'get_site',
    { siteId },
    {
      successFeedback: 'none',
      failFeedback: 'none',
    },
  );
}

/** 连接站点并获取模型目录，返回新站点 ID。 */
export function createSite(input: CreateSiteInput): Promise<string> {
  return callBackend<string>(
    'create_site',
    { input },
    { successText: '站点已添加', failFeedback: 'message' },
  );
}

/** 保存站点基本信息，返回最新配置；凭据仅以配置状态返回。 */
export function saveSite(siteId: string, draft: SiteDraft): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'save_site',
    { siteId, draft },
    { successText: '站点配置已保存', failFeedback: 'message' },
  );
}

/** 单独保存高级连接配置，不修改基本信息、凭据或探针策略。 */
export function saveSiteConnection(siteId: string, connection: Connection): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'update_site_connection',
    { siteId, connection },
    { successText: '高级设置已保存', failFeedback: 'message' },
  );
}

/** 删除站点及其模型配置。 */
export function deleteSite(siteId: string): Promise<void> {
  return callBackend<void>(
    'delete_site',
    { siteId },
    {
      successText: '站点已删除',
      failFeedback: 'message',
    },
  );
}

/** 按完整站点 ID 列表保存优先级；失败保留原顺序。 */
export function reorderSites(ids: string[]): Promise<SiteListItem[]> {
  return callBackend<SiteListItem[]>(
    'reorder_sites',
    { ids },
    {
      successFeedback: 'none',
      failFeedback: 'message',
    },
  );
}

/** 设置站点是否在小面板中显示，不影响模型探测。 */
export function toggleSite(siteId: string, enabled: boolean): Promise<SiteListItem> {
  return callBackend<SiteListItem>(
    'set_site_enabled',
    { siteId, enabled },
    {
      failFeedback: 'message',
    },
  );
}

/** 手动添加模型。 */
export function addModel(siteId: string, name: string): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'add_site_model',
    { siteId, name },
    {
      successText: '模型已添加',
      failFeedback: 'message',
    },
  );
}

/** 删除模型及其独立探针设置。 */
export function deleteModel(siteId: string, modelId: string): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'delete_site_model',
    { siteId, modelId },
    {
      successText: '模型已删除',
      failFeedback: 'message',
    },
  );
}

/** 设置模型启用状态。 */
export function toggleModel(
  siteId: string,
  modelId: string,
  enabled: boolean,
): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'set_site_model_enabled',
    { siteId, modelId, enabled },
    {
      failFeedback: 'message',
    },
  );
}

/** 获取远端模型目录并检查差异，不保存；models 是当前站点的完整模型名称数组。 */
export function syncModels(siteId: string, models: string[]): Promise<ModelSyncChanges> {
  return callBackend<ModelSyncChanges>(
    'sync_site_models',
    { siteId, models },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

/** 写入用户确认的模型差异；成功后由页面重新读取站点详情。 */
export function applyModelSync(siteId: string, input: ApplyModelSyncInput): Promise<void> {
  return callBackend<void>(
    'apply_site_model_sync',
    { siteId, ...input },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

/** 用当前草稿试探一次，继承站点凭据和请求头；不保存配置或覆盖正式探测结果。 */
export function testCustomProbe(
  siteId: string,
  modelId: string,
  request: CustomProbeRequest,
): Promise<CustomProbeTestResult> {
  return callBackend<CustomProbeTestResult>(
    'test_model_probe',
    { siteId, modelId, request },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}

/** 保存站点默认策略或模型独立策略；模型 policy 为 null 时继承站点。 */
export function savePolicy(
  siteId: string,
  modelId: string | null,
  policy: ProbePolicy | null,
  customRequest: SiteModel['customRequest'],
): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'update_site_probe_policy',
    { siteId, modelId, policy, customRequest },
    { successText: '探针设置已保存', failFeedback: 'message' },
  );
}

/** 手动探测一个模型，或站点下全部启用模型。 */
export function runProbe(siteId: string, modelId?: string): Promise<ManagedSite> {
  return callBackend<ManagedSite>(
    'run_site_probe',
    { siteId, modelId },
    {
      successText: '探测已完成',
      failFeedback: 'message',
    },
  );
}
