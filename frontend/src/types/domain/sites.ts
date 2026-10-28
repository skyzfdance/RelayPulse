/** 站点及其模型的健康状态。 */
export type HealthStatus = 'healthy' | 'warning' | 'error' | 'muted';
export type ParameterType = 'string' | 'boolean' | 'number' | 'object' | 'json';
export type SiteProtocol = 'openai-compatible' | 'anthropic' | 'openai-responses' | 'custom-http';

export interface RequestParameter {
  key: string;
  value: string;
  type: ParameterType;
  enabled: boolean;
  binding?: 'model';
}

export interface RequestHeader {
  key: string;
  value: string;
}

/** 添加站点的输入；连接配置与探针策略由创建接口初始化。 */
export interface CreateSiteInput {
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  secret: string;
  /** 是否在小面板中显示，不影响模型探测。 */
  enabled: boolean;
}

/** 接口提供的可用协议目录，名称及排列顺序由服务端决定。 */
export interface SiteProtocolOption {
  value: SiteProtocol;
  label: string;
}

export interface ProbePolicy {
  version: 2;
  parameters: RequestParameter[];
}

export interface CustomProbeRequest {
  version: 2;
  endpoint: string;
  parameters: RequestParameter[];
}

export interface CustomProbeTestResult {
  success: boolean;
  statusCode: number | null;
  durationMs: number;
  message: string;
  simulated: boolean;
}

export interface Connection {
  modelsPath: string;
  probePath: string;
  intervalSeconds: number;
  slowFirstTokenSeconds: number;
  slowTotalSeconds: number;
  failureThreshold: number;
  recoveryThreshold: number;
  headers: RequestHeader[];
}

export interface SiteModel {
  id: string;
  name: string;
  enabled: boolean;
  health: HealthStatus;
  firstTokenMs: number | null;
  durationMs: number | null;
  policy: ProbePolicy | null;
  customRequest: CustomProbeRequest | null;
}

/** 左侧列表摘要，不包含连接配置、凭据或模型明细。priority 从 1 开始。 */
export interface SiteListItem {
  id: string;
  name: string;
  priority: number;
  /** 是否在小面板中显示，不影响模型探测。 */
  enabled: boolean;
  modelCount: number;
  health: HealthStatus;
  statusText: string;
}

export interface ManagedSite {
  id: string;
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  /** 是否在小面板中显示，不影响模型探测。 */
  enabled: boolean;
  credentialConfigured: boolean;
  connection: Connection;
  policy: ProbePolicy;
  models: SiteModel[];
}

export interface SiteDraft {
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  credentialAction: 'keep' | 'replace';
  secret: string;
}

/** 同步检查只返回差异，不修改已保存模型。新增项尚无本地 ID。 */
export interface ModelSyncChanges {
  added: string[];
  removed: { id: string; name: string }[];
}

/** 确认后的写入内容；models 保留检查时的完整模型名称集合，用于检查目录是否已变化。 */
export interface ApplyModelSyncInput {
  models: string[];
  added: string[];
  /** 仅删除记录使用内部 ID，不用于远端模型匹配。 */
  removed: string[];
}
