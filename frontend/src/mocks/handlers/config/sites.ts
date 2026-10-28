/**
 * 站点管理命令的浏览器 Mock（config/sites）。
 * 本模块维护站点会话数据和命令处理，修改仅保留在当前会话。
 */
import type { MockHandler } from '../registry';
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
  SiteProtocol,
} from '@/types/domain/sites';

import { RelayPulseBackendError } from '@/utils/backend-error';
import { validateConnection } from '@/views/config/sites/connection-validation';
import { customProbeError, policyError } from '@/views/config/sites/probe-policy';
import { defaultProbePolicy } from '@/views/config/sites/probe-preview';
import { validateSite } from '@/views/config/sites/site-policy';

import { registerMockHandler } from '../registry';

/** 浏览器 Mock 的入参可能是 Vue 响应式对象；只复制契约中的 JSON 数据。 */
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));
const id = () => crypto.randomUUID();
export const state: { sites: ManagedSite[] } = { sites: [] };
function requireSite(siteId: string) {
  const site = state.sites.find((s) => s.id === siteId);
  if (!site) throw businessError('站点不存在，请刷新列表');
  return site;
}
function businessError(message: string) {
  return new RelayPulseBackendError({
    code: 'site_validation',
    category: 'validation',
    message,
    retryable: false,
  });
}
function assertAvailable(write = false) {
  const scenario = new URLSearchParams(window.location.search).get('scenario');
  if (scenario === 'error' || (write && scenario === 'action-error'))
    throw new RelayPulseBackendError({
      code: 'site_unavailable',
      category: 'unavailable',
      message: '暂时无法完成操作，请稍后重试',
      retryable: true,
    });
}
/** 浏览器站点操作（含同步检查）与试探默认延迟 1 秒；单测按需通过 ?siteDelay=毫秒 开启同样的延迟。 */
function registerSiteOperation(command: string, handler: MockHandler) {
  registerMockHandler(command, async (args) => {
    const input = clone(args);
    const configured = new URLSearchParams(window.location.search).get('siteDelay');
    const defaultDelay = import.meta.env.MODE === 'test' ? 0 : 1000;
    const delay = configured === null ? defaultDelay : Number(configured);
    if (Number.isFinite(delay) && delay > 0) {
      await new Promise((resolve) => setTimeout(resolve, Math.min(delay, 10000)));
    }
    return handler(input);
  });
}

function defaultConnection(protocol: SiteProtocol): Connection {
  const paths: Record<SiteProtocol, string> = {
    'openai-compatible': '/v1/chat/completions',
    'openai-responses': '/v1/responses',
    anthropic: '/v1/messages',
    'custom-http': '/probe',
  };
  return {
    modelsPath: '/v1/models',
    probePath: paths[protocol],
    intervalSeconds: 300,
    slowFirstTokenSeconds: 5,
    slowTotalSeconds: 30,
    failureThreshold: 3,
    recoveryThreshold: 2,
    headers: [],
  };
}

export function seedSites(empty = false) {
  state.sites = empty
    ? []
    : ['OpenAI 主站', 'Anthropic', '备用线路'].map((name, i): ManagedSite => ({
        id: `site-${i + 1}`,
        name,
        protocol: i === 1 ? 'anthropic' : 'openai-compatible',
        baseUrl: [
          'https://api.openai.com',
          'https://api.anthropic.com',
          'https://relay.example.com',
        ][i],
        enabled: true,
        credentialConfigured: true,
        connection: defaultConnection(i === 1 ? 'anthropic' : 'openai-compatible'),
        policy: defaultProbePolicy(i === 1 ? 'anthropic' : 'openai-compatible'),
        models: (i === 1
          ? ['claude-sonnet-demo', 'claude-opus-demo']
          : ['gpt-demo', 'gpt-mini-demo', 'legacy-demo']
        ).map((model, j) => ({
          id: `model-${i}-${j}`,
          name: model,
          enabled: j < 2,
          health: (['healthy', 'healthy', 'warning'] as const)[i],
          firstTokenMs: j === 2 ? null : 350 + i * 120,
          durationMs: j === 2 ? null : 1200 + i * 200,
          policy: null,
          customRequest: null,
        })),
      }));
  for (const site of state.sites)
    for (const model of site.models) if (!model.enabled) model.health = 'muted';
}
seedSites(new URLSearchParams(window.location.search).get('scenario') === 'empty');

/** 模拟接口聚合状态，页面只消费摘要。 */
function listItem(site: ManagedSite): SiteListItem {
  const enabledModels = site.models.filter((model) => model.enabled);
  const errors = enabledModels.filter((model) => model.health === 'error').length;
  let health: SiteListItem['health'] = 'healthy';
  let statusText = '全部正常';
  if (!enabledModels.length) {
    health = 'muted';
    statusText = '没有启用模型';
  } else if (errors) {
    health = 'error';
    statusText = `${errors} 个异常`;
  } else if (enabledModels.some((model) => model.health === 'warning')) {
    health = 'warning';
    statusText = '响应偏慢';
  } else if (enabledModels.some((model) => model.health === 'muted')) {
    health = 'muted';
    statusText = '等待探测';
  }
  return {
    id: site.id,
    name: site.name,
    priority: state.sites.indexOf(site) + 1,
    enabled: site.enabled,
    modelCount: site.models.length,
    health,
    statusText,
  };
}
registerMockHandler('get_site', (args) => {
  assertAvailable();
  const { siteId } = args as { siteId: string };
  return clone(requireSite(siteId));
});
registerMockHandler('get_sites', () => {
  assertAvailable();
  return state.sites.map(listItem);
});
registerSiteOperation('create_site', (args) => {
  assertAvailable(true);
  const { input } = args as { input: CreateSiteInput };
  const connection = defaultConnection(input.protocol);
  const error = validateSite({
    name: input.name,
    protocol: input.protocol,
    baseUrl: input.baseUrl,
    credentialAction: 'replace',
    secret: input.secret,
  });
  if (error) throw businessError(error);
  const site: ManagedSite = {
    id: id(),
    name: input.name.trim(),
    protocol: input.protocol,
    baseUrl: input.baseUrl.trim().replace(/\/$/u, ''),
    enabled: input.enabled,
    credentialConfigured: true,
    connection,
    policy: defaultProbePolicy(input.protocol),
    // 仅模拟连接与模型发现，不发送网络请求，也不保存输入凭据。
    models: remoteModelNames(input.protocol).map(newModel),
  };
  state.sites.push(site);
  return site.id;
});
registerSiteOperation('save_site', (args) => {
  assertAvailable(true);
  const { siteId, draft } = args as { siteId: string; draft: SiteDraft };
  const error = validateSite(draft);
  if (error) throw businessError(error);
  const site = requireSite(siteId);
  site.name = draft.name.trim();
  site.protocol = draft.protocol;
  site.baseUrl = draft.baseUrl.trim().replace(/\/$/u, '');
  if (draft.credentialAction !== 'keep')
    site.credentialConfigured = draft.credentialAction === 'replace';
  // Mock 只保存是否已配置，绝不保存、记录或回显输入的 secret。
  return clone(site);
});
registerSiteOperation('update_site_connection', (args) => {
  assertAvailable(true);
  const { siteId, connection } = args as { siteId: string; connection: Connection };
  const site = requireSite(siteId);
  const error = validateConnection(connection, site.baseUrl);
  if (error) throw businessError(error);
  site.connection = clone(connection);
  return clone(site);
});
registerSiteOperation('delete_site', (args) => {
  assertAvailable(true);
  const { siteId } = args as { siteId: string };
  requireSite(siteId);
  state.sites = state.sites.filter((s) => s.id !== siteId);
});
registerSiteOperation('reorder_sites', (args) => {
  assertAvailable(true);
  const { ids } = args as { ids: string[] };
  if (
    ids.length !== state.sites.length ||
    new Set(ids).size !== ids.length ||
    ids.some((key) => !state.sites.some((s) => s.id === key))
  )
    throw businessError('站点列表已变化，请刷新后重新排序');
  state.sites = ids.map(requireSite);
  return state.sites.map(listItem);
});
registerSiteOperation('set_site_enabled', (args) => {
  assertAvailable(true);
  const { siteId, enabled } = args as { siteId: string; enabled: boolean };
  const site = requireSite(siteId);
  site.enabled = enabled;
  return listItem(site);
});
function requireModel(siteId: string, modelId: string) {
  const model = requireSite(siteId).models.find((m) => m.id === modelId);
  if (!model) throw businessError('模型不存在');
  return model;
}
registerSiteOperation('add_site_model', (args) => {
  assertAvailable(true);
  const { siteId, name } = args as { siteId: string; name: string };
  const site = requireSite(siteId);
  if (!name.trim() || site.models.some((m) => m.name === name.trim()))
    throw businessError('模型名称不能为空或重复');
  site.models.push(newModel(name.trim()));
  return clone(site);
});
registerSiteOperation('delete_site_model', (args) => {
  assertAvailable(true);
  const { siteId, modelId } = args as { siteId: string; modelId: string };
  requireModel(siteId, modelId);
  const site = requireSite(siteId);
  site.models = site.models.filter((m) => m.id !== modelId);
  return clone(site);
});
registerSiteOperation('set_site_model_enabled', (args) => {
  assertAvailable(true);
  const { siteId, modelId, enabled } = args as {
    siteId: string;
    modelId: string;
    enabled: boolean;
  };
  const model = requireModel(siteId, modelId);
  model.enabled = enabled;
  return clone(requireSite(siteId));
});
function remoteModelNames(protocol: ManagedSite['protocol']) {
  return protocol === 'anthropic'
    ? ['claude-sonnet-demo', 'claude-opus-demo']
    : ['gpt-demo', 'gpt-mini-demo'];
}
function newModel(name: string): SiteModel {
  return {
    id: id(),
    name,
    enabled: true,
    health: 'muted',
    firstTokenMs: null,
    durationMs: null,
    policy: null,
    customRequest: null,
  };
}
/** 检查传入的完整模型名称集合，拒绝使用过期列表检查或确认删除。 */
function assertCurrentModels(site: ManagedSite, models: string[]) {
  if (
    !Array.isArray(models) ||
    models.length !== site.models.length ||
    new Set(models).size !== models.length ||
    models.some((name) => !site.models.some((model) => model.name === name))
  )
    throw businessError('模型列表已变化，请刷新后重新检查同步');
}
registerSiteOperation('sync_site_models', (args): ModelSyncChanges => {
  assertAvailable();
  const { siteId, models } = args as { siteId: string; models: string[] };
  const site = requireSite(siteId);
  assertCurrentModels(site, models);
  if (!site.credentialConfigured) throw businessError('请先配置访问凭据');
  const remote = new Set(remoteModelNames(site.protocol));
  return {
    added: [...remote].filter((name) => !models.includes(name)),
    removed: site.models
      .filter((model) => !remote.has(model.name))
      .map(({ id, name }) => ({ id, name })),
  };
});
registerSiteOperation('apply_site_model_sync', (args) => {
  assertAvailable(true);
  const { siteId, models, added, removed } = args as ApplyModelSyncInput & { siteId: string };
  const site = requireSite(siteId);
  assertCurrentModels(site, models);
  if (
    !Array.isArray(added) ||
    !Array.isArray(removed) ||
    new Set(added).size !== added.length ||
    new Set(removed).size !== removed.length ||
    added.some(
      (name) =>
        typeof name !== 'string' ||
        !name.trim() ||
        name !== name.trim() ||
        site.models.some((model) => model.name === name),
    ) ||
    removed.some((modelId) => !site.models.some((model) => model.id === modelId))
  )
    throw businessError('同步差异无效，请重新检查同步');
  // 只写入用户确认的差异，不再次读取远端，不改动保留模型的独立设置。
  const next = site.models.filter((model) => !removed.includes(model.id));
  site.models = [...next, ...added.map(newModel)];
});
registerSiteOperation('test_model_probe', (args): CustomProbeTestResult => {
  assertAvailable(true);
  const { siteId, modelId, request } = args as {
    siteId: string;
    modelId: string;
    request: CustomProbeRequest;
  };
  const site = requireSite(siteId);
  requireModel(siteId, modelId);
  const error = customProbeError(request, site.baseUrl);
  if (error) throw businessError(error);
  if (!site.credentialConfigured) throw businessError('请先配置访问凭据');
  // 浏览器仅模拟试探，不写入配置和正式探测记录；probeStatus 可模拟 HTTP 错误。
  const configured = Number(new URLSearchParams(window.location.search).get('probeStatus') ?? 200);
  const statusCode =
    Number.isInteger(configured) && configured >= 100 && configured <= 599 ? configured : 200;
  return {
    success: statusCode >= 200 && statusCode < 300,
    statusCode,
    durationMs: 1250,
    message: '当前为 Mock 模式，未向站点发送真实请求。',
    simulated: true,
  };
});
registerSiteOperation('update_site_probe_policy', (args) => {
  assertAvailable(true);
  const { siteId, modelId, policy, customRequest } = args as {
    siteId: string;
    modelId: string | null;
    policy: ProbePolicy | null;
    customRequest: SiteModel['customRequest'];
  };
  if (policy) {
    const error = policyError(policy);
    if (error) throw businessError(error);
  }
  if (customRequest) {
    if (!modelId || policy) throw businessError('自定义请求仅用于模型，不能与常规探针策略同时保存');
    const error = customProbeError(customRequest, requireSite(siteId).baseUrl);
    if (error) throw businessError(error);
  }
  if (modelId) {
    const model = requireModel(siteId, modelId);
    model.policy = clone(policy);
    model.customRequest = clone(customRequest);
  } else {
    if (!policy) throw businessError('站点必须有默认探针策略');
    requireSite(siteId).policy = clone(policy);
  }
  return clone(requireSite(siteId));
});
registerSiteOperation('run_site_probe', (args) => {
  assertAvailable(true);
  const { siteId, modelId } = args as { siteId: string; modelId?: string };
  const site = requireSite(siteId);
  if (!site.credentialConfigured) throw businessError('请先配置访问凭据');
  const models = modelId ? [requireModel(siteId, modelId)] : site.models.filter((m) => m.enabled);
  if (!models.length) throw businessError('没有可探测的模型');
  for (const model of models) {
    model.firstTokenMs = model.customRequest ? null : 420;
    model.durationMs = 1250;
    model.health = 'healthy';
  }
  return clone(site);
});
