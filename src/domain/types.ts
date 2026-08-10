export const viewNames = [
  'sites',
  'add-site',
  'site-advanced',
  'model-settings',
  'general',
  'notifications',
  'records',
  'operation-history',
  'intelligence-ladder',
  'delete-confirm',
  'empty',
  'tray-healthy',
  'tray-alerts',
] as const;

export type ViewName = (typeof viewNames)[number];
export type HealthStatus = 'healthy' | 'warning' | 'error' | 'muted';
export type ThemeMode = 'system' | 'light' | 'dark';
export type ProbeMode = 'inherit' | 'disabled-thinking' | 'minimum-reasoning';
export type ParameterType = 'string' | 'boolean' | 'number' | 'json';
export type ProbeRecordStatus = 'healthy' | 'warning' | 'error';
export type SiteProtocol = 'openai-compatible' | 'anthropic' | 'openai-responses' | 'custom-http';

export type CredentialIntent =
  { action: 'keep' } | { action: 'replace'; secret: string } | { action: 'clear' };

export interface CredentialSummary {
  configured: boolean;
  masked: string | null;
}

export interface PassThroughParameter {
  key: string;
  value: string;
  type: ParameterType;
}

export interface ProbeSettings {
  mode: ProbeMode;
  maxOutputTokens: number;
  prompt: string;
  parameters: PassThroughParameter[];
}

export interface CustomProbeSettings {
  enabled: boolean;
  endpoint: string;
  parameters: PassThroughParameter[];
}

export interface Model {
  id: string;
  name: string;
  enabled: boolean;
  status: HealthStatus;
  inferenceSummary: string;
  firstToken: string;
  totalTime: string;
  latestProbeId?: string | null;
  nextProbeAt?: string | null;
  capabilities: string[];
  probeSettings: ProbeSettings;
  customProbe: CustomProbeSettings;
}

export interface ModelSyncRemoval {
  id: string;
  name: string;
}

export type ModelSyncResult =
  | { status: 'applied'; snapshot: AppSnapshot }
  | { status: 'confirmationRequired'; removedModels: ModelSyncRemoval[] };

export interface RequestHeader {
  key: string;
  value: string;
}

export interface Site {
  id: string;
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  credential: CredentialSummary;
  enabled: boolean;
  priority: number;
  status: HealthStatus;
  summary: string;
  modelsEndpoint: string;
  probeEndpoint: string;
  probeIntervalMinutes: number | null;
  slowFirstTokenSeconds: number;
  slowTotalSeconds: number;
  failureThreshold: number;
  recoveryThreshold: number;
  headers: RequestHeader[];
  defaultProbeSettings: ProbeSettings;
  models: Model[];
}

export interface GeneralSettings {
  launchAtStartup: boolean;
  continueInBackground: boolean;
  trayAction: 'panel' | 'disabled';
  theme: ThemeMode;
  language: 'zh-CN';
  historyRetentionDays: number;
  diagnosticCaptureEnabled: boolean;
}

export interface TrayAlert {
  id: string;
  siteName: string;
  modelName: string;
  status: 'warning' | 'error';
  firstToken: string;
  totalTime: string;
}

export interface TraySnapshot {
  status: 'healthy' | 'warning' | 'error' | 'muted';
  activeSites: number;
  activeModels: number;
  alertCount: number;
  alerts: TrayAlert[];
  completedLabel: string;
  nextProbeLabel: string;
}

export interface TokenUsage {
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  source: 'reported' | 'unavailable';
}

export type ProbeResponseMode = 'stream' | 'synchronous';

export interface ProbeCapture {
  headers: RequestHeader[];
  rawBody: string;
  capturedBytes: number;
  captureLimitBytes: number;
  truncated: boolean;
}

export interface ProbeResponseChunk {
  sequence: number;
  receivedAfterMs: number;
  capturedBytes: number;
  raw: string;
}

export interface ProbeExchange {
  request: ProbeCapture;
  response: ProbeCapture & {
    mode: ProbeResponseMode;
    contentType: string | null;
    chunks: ProbeResponseChunk[];
  };
}

export interface ProbeRecord {
  id: string;
  startedAt: string;
  status: ProbeRecordStatus;
  siteId: string;
  siteName: string;
  modelId: string;
  modelName: string;
  firstTokenMs: number | null;
  firstTokenLabel: string;
  totalTimeMs: number;
  totalTimeLabel: string;
  result: string;
  method: 'GET' | 'POST';
  endpoint: string;
  probeMode: string;
  httpStatus: number | null;
  requestId: string | null;
  errorDetail: string | null;
  tokenUsage: TokenUsage;
  exchange: ProbeExchange;
}

export interface AppSnapshot {
  revision: number;
  sites: Site[];
  selectedSiteId: string | null;
  generalSettings: GeneralSettings;
  probeRecords: ProbeRecord[];
  tray: {
    healthy: TraySnapshot;
    alerts: TraySnapshot;
  };
}

export interface CreateSiteInput {
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  credential: Extract<CredentialIntent, { action: 'replace' }>;
  enabled: boolean;
}

export interface CreateModelInput {
  name: string;
  enabled: boolean;
}

export interface UpdateSiteInput {
  name: string;
  protocol: SiteProtocol;
  baseUrl: string;
  credential: CredentialIntent;
}

export interface SiteAdvancedInput {
  modelsEndpoint: string;
  probeEndpoint: string;
  probeIntervalMinutes: number | null;
  slowFirstTokenSeconds: number;
  slowTotalSeconds: number;
  failureThreshold: number;
  recoveryThreshold: number;
  headers: RequestHeader[];
}
