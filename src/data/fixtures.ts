import type {
  AppSnapshot,
  Model,
  ProbeExchange,
  ProbeRecord,
  ProbeResponseMode,
  ProbeSettings,
  Site,
  TokenUsage,
} from '../domain/types';

const siteDefaultProbeSettings: ProbeSettings = {
  mode: 'disabled-thinking',
  maxOutputTokens: 8,
  prompt: '只回复 OK',
  parameters: [],
};

const inheritedProbeSettings: ProbeSettings = {
  ...siteDefaultProbeSettings,
  mode: 'inherit',
};

function model(
  id: string,
  name: string,
  enabled: boolean,
  status: Model['status'],
  inferenceSummary: string,
  firstToken: string,
  totalTime: string,
  settings: Partial<ProbeSettings> = {},
): Model {
  return {
    id,
    name,
    enabled,
    status,
    inferenceSummary,
    firstToken,
    totalTime,
    capabilities: ['自动识别', '支持关闭思考', '支持 low / medium / high'],
    probeSettings: { ...inheritedProbeSettings, ...settings },
    customProbe: {
      enabled: false,
      endpoint: '',
      parameters: [],
    },
  };
}

const sites: Site[] = [
  {
    id: 'openai-main',
    name: 'OpenAI 主线路',
    protocol: 'openai-compatible',
    baseUrl: 'https://api.openai.com/v1',
    credential: { configured: true, masked: 'sk-proj-••••••••••••••••' },
    enabled: true,
    priority: 1,
    status: 'healthy',
    summary: '6 个模型 · 全部正常',
    modelsEndpoint: '/models',
    probeEndpoint: '/chat/completions',
    probeIntervalMinutes: 10,
    firstTokenTimeoutSeconds: 8,
    totalTimeoutSeconds: 30,
    headers: [{ key: 'X-Workspace', value: 'relaypulse-prod' }],
    defaultProbeSettings: siteDefaultProbeSettings,
    models: [
      model(
        'gpt-5-4-mini',
        'gpt-5.4-mini',
        true,
        'healthy',
        '自动 · 关闭思考',
        '278 ms',
        '1.12 s',
        { mode: 'disabled-thinking' },
      ),
      model('gpt-5-4', 'gpt-5.4', true, 'healthy', '自动 · 最低推理', '421 ms', '1.84 s', {
        mode: 'disabled-thinking',
        parameters: [
          { key: 'reasoning_effort', value: 'none', type: 'string' },
          { key: 'store', value: 'false', type: 'boolean' },
        ],
      }),
      model('o3-mini', 'o3-mini', true, 'warning', '手动 · low', '693 ms', '2.36 s', {
        mode: 'minimum-reasoning',
      }),
      model('gpt-4-1-mini', 'gpt-4.1-mini', false, 'muted', '继承站点默认', '—', '—'),
    ],
  },
  {
    id: 'claude-backup',
    name: 'Claude 备用线路',
    protocol: 'openai-compatible',
    baseUrl: 'https://claude-gateway.example/v1',
    credential: { configured: true, masked: 'sk-ant-••••••••••••••••' },
    enabled: true,
    priority: 2,
    status: 'error',
    summary: '4 个模型 · 1 个异常',
    modelsEndpoint: '/models',
    probeEndpoint: '/chat/completions',
    probeIntervalMinutes: 20,
    firstTokenTimeoutSeconds: 8,
    totalTimeoutSeconds: 30,
    headers: [],
    defaultProbeSettings: siteDefaultProbeSettings,
    models: [
      model('claude-sonnet', 'claude-3-7-sonnet', true, 'error', '最低推理', '超时', '8.00 s'),
    ],
  },
  {
    id: 'gemini-asia',
    name: 'Gemini 亚洲线路',
    protocol: 'openai-compatible',
    baseUrl: 'https://gemini-gateway.example/v1',
    credential: { configured: true, masked: 'gm-••••••••••••••••' },
    enabled: true,
    priority: 3,
    status: 'warning',
    summary: '3 个模型 · 响应偏慢',
    modelsEndpoint: '/models',
    probeEndpoint: '/chat/completions',
    probeIntervalMinutes: 10,
    firstTokenTimeoutSeconds: 8,
    totalTimeoutSeconds: 30,
    headers: [],
    defaultProbeSettings: siteDefaultProbeSettings,
    models: [
      model('gemini-pro', 'gemini-2.5-pro', true, 'warning', '继承站点默认', '1.82 s', '6.41 s'),
    ],
  },
  {
    id: 'ollama-local',
    name: '本地 Ollama',
    protocol: 'openai-compatible',
    baseUrl: 'http://127.0.0.1:11434/v1',
    credential: { configured: false, masked: null },
    enabled: true,
    priority: 4,
    status: 'muted',
    summary: '0 个模型启用 · 不参与探测',
    modelsEndpoint: '/models',
    probeEndpoint: '/chat/completions',
    probeIntervalMinutes: 30,
    firstTokenTimeoutSeconds: 8,
    totalTimeoutSeconds: 30,
    headers: [],
    defaultProbeSettings: siteDefaultProbeSettings,
    models: [],
  },
  {
    id: 'legacy-proxy',
    name: '旧代理节点',
    protocol: 'openai-compatible',
    baseUrl: 'https://legacy.example/v1',
    credential: { configured: true, masked: 'sk-old-••••••••••••' },
    enabled: false,
    priority: 5,
    status: 'muted',
    summary: '站点已停用',
    modelsEndpoint: '/models',
    probeEndpoint: '/chat/completions',
    probeIntervalMinutes: 60,
    firstTokenTimeoutSeconds: 8,
    totalTimeoutSeconds: 30,
    headers: [],
    defaultProbeSettings: siteDefaultProbeSettings,
    models: [],
  },
];

const noTokenUsage: TokenUsage = {
  inputTokens: null,
  outputTokens: null,
  totalTokens: null,
  source: 'unavailable',
};

function reportedTokenUsage(inputTokens: number, outputTokens: number): TokenUsage {
  return {
    inputTokens,
    outputTokens,
    totalTokens: inputTokens + outputTokens,
    source: 'reported',
  };
}

function capturedBytes(value: string) {
  return new TextEncoder().encode(value).byteLength;
}

function probeExchange(
  record: Pick<
    ProbeRecord,
    'id' | 'modelName' | 'firstTokenMs' | 'totalTimeMs' | 'httpStatus' | 'tokenUsage'
  >,
  mode: ProbeResponseMode,
): ProbeExchange {
  const requestBody = JSON.stringify(
    {
      model: record.modelName,
      messages: [{ role: 'user', content: '只回复 OK' }],
      stream: mode === 'stream',
      max_tokens: 8,
    },
    null,
    2,
  );
  const request = {
    headers: [
      { key: 'Content-Type', value: 'application/json' },
      {
        key: 'Accept',
        value: mode === 'stream' ? 'text/event-stream' : 'application/json',
      },
      { key: 'Authorization', value: 'Bearer ••••••••••••••••' },
    ],
    rawBody: requestBody,
    capturedBytes: capturedBytes(requestBody),
    captureLimitBytes: 32 * 1024,
    truncated: false,
  };

  if (record.httpStatus === null) {
    return {
      request,
      response: {
        mode,
        contentType: null,
        headers: [],
        rawBody: '',
        chunks: [],
        capturedBytes: 0,
        captureLimitBytes: 64 * 1024,
        truncated: false,
      },
    };
  }

  if (mode === 'stream') {
    const streamId = `chatcmpl_${record.id}`;
    const chunks = [
      {
        sequence: 1,
        receivedAfterMs: record.firstTokenMs ?? record.totalTimeMs,
        raw: `data: ${JSON.stringify({
          id: streamId,
          choices: [{ index: 0, delta: { content: 'OK' } }],
        })}\n\n`,
      },
      {
        sequence: 2,
        receivedAfterMs: record.totalTimeMs,
        raw: `data: ${JSON.stringify({
          id: streamId,
          choices: [],
          usage:
            record.tokenUsage.source === 'reported'
              ? {
                  prompt_tokens: record.tokenUsage.inputTokens,
                  completion_tokens: record.tokenUsage.outputTokens,
                  total_tokens: record.tokenUsage.totalTokens,
                }
              : null,
        })}\n\ndata: [DONE]\n\n`,
      },
    ].map((chunk) => ({ ...chunk, capturedBytes: capturedBytes(chunk.raw) }));
    const rawBody = chunks.map((chunk) => chunk.raw).join('');
    return {
      request,
      response: {
        mode,
        contentType: 'text/event-stream',
        headers: [
          { key: 'Content-Type', value: 'text/event-stream; charset=utf-8' },
          { key: 'Cache-Control', value: 'no-cache' },
        ],
        rawBody,
        chunks,
        capturedBytes: capturedBytes(rawBody),
        captureLimitBytes: 64 * 1024,
        truncated: false,
      },
    };
  }

  const rawBody = JSON.stringify(
    record.httpStatus >= 400
      ? {
          error: {
            type: 'rate_limit_exceeded',
            message: 'Request rate limit exceeded.',
          },
        }
      : {
          id: `response_${record.id}`,
          model: record.modelName,
          choices: [{ index: 0, message: { role: 'assistant', content: 'OK' } }],
          usage:
            record.tokenUsage.source === 'reported'
              ? {
                  prompt_tokens: record.tokenUsage.inputTokens,
                  completion_tokens: record.tokenUsage.outputTokens,
                  total_tokens: record.tokenUsage.totalTokens,
                }
              : null,
        },
    null,
    2,
  );
  return {
    request,
    response: {
      mode,
      contentType: 'application/json',
      headers: [{ key: 'Content-Type', value: 'application/json; charset=utf-8' }],
      rawBody,
      chunks: [],
      capturedBytes: capturedBytes(rawBody),
      captureLimitBytes: 64 * 1024,
      truncated: false,
    },
  };
}

function probeRecord(
  input: Omit<
    ProbeRecord,
    'method' | 'probeMode' | 'httpStatus' | 'requestId' | 'errorDetail' | 'exchange'
  > &
    Partial<Pick<ProbeRecord, 'probeMode' | 'httpStatus' | 'requestId' | 'errorDetail'>> & {
      responseMode?: ProbeResponseMode;
    },
): ProbeRecord {
  const { responseMode = 'stream', ...recordInput } = input;
  const record = {
    method: 'POST',
    probeMode: '站点默认 · 关闭思考',
    httpStatus: 200,
    requestId: `rp_${input.id}`,
    errorDetail: null,
    ...recordInput,
  } satisfies Omit<ProbeRecord, 'exchange'>;
  return { ...record, exchange: probeExchange(record, responseMode) };
}

const probeRecords: ProbeRecord[] = [
  probeRecord({
    id: 'probe-001',
    startedAt: '2026-07-30T14:10:42+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4-mini',
    modelName: 'gpt-5.4-mini',
    firstTokenMs: 278,
    firstTokenLabel: '278 ms',
    totalTimeMs: 1120,
    totalTimeLabel: '1.12 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(11, 2),
  }),
  probeRecord({
    id: 'probe-002',
    startedAt: '2026-07-30T14:10:39+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4',
    modelName: 'gpt-5.4',
    firstTokenMs: 421,
    firstTokenLabel: '421 ms',
    totalTimeMs: 1840,
    totalTimeLabel: '1.84 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(12, 2),
  }),
  probeRecord({
    id: 'probe-003',
    startedAt: '2026-07-30T14:09:58+08:00',
    status: 'error',
    siteId: 'claude-backup',
    siteName: 'Claude 备用线路',
    modelId: 'claude-sonnet',
    modelName: 'claude-3-7-sonnet',
    firstTokenMs: null,
    firstTokenLabel: '超时',
    totalTimeMs: 8000,
    totalTimeLabel: '8.00 s',
    result: '首 Token 超时',
    endpoint: 'https://claude-gateway.example/v1/chat/completions',
    httpStatus: null,
    requestId: null,
    errorDetail: '在 8 秒内未收到首个响应事件，连接已由 RelayPulse 终止。',
    tokenUsage: noTokenUsage,
  }),
  probeRecord({
    id: 'probe-004',
    startedAt: '2026-07-30T14:09:41+08:00',
    status: 'warning',
    siteId: 'gemini-asia',
    siteName: 'Gemini 亚洲线路',
    modelId: 'gemini-pro',
    modelName: 'gemini-2.5-pro',
    firstTokenMs: 1820,
    firstTokenLabel: '1.82 s',
    totalTimeMs: 6410,
    totalTimeLabel: '6.41 s',
    result: '响应偏慢',
    endpoint: 'https://gemini-gateway.example/v1/chat/completions',
    responseMode: 'synchronous',
    tokenUsage: reportedTokenUsage(14, 2),
  }),
  probeRecord({
    id: 'probe-005',
    startedAt: '2026-07-30T14:08:43+08:00',
    status: 'warning',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'o3-mini',
    modelName: 'o3-mini',
    firstTokenMs: 693,
    firstTokenLabel: '693 ms',
    totalTimeMs: 2360,
    totalTimeLabel: '2.36 s',
    result: '响应偏慢',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    probeMode: '模型覆盖 · 最低推理',
    tokenUsage: reportedTokenUsage(13, 2),
  }),
  probeRecord({
    id: 'probe-006',
    startedAt: '2026-07-30T14:06:42+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4-mini',
    modelName: 'gpt-5.4-mini',
    firstTokenMs: 266,
    firstTokenLabel: '266 ms',
    totalTimeMs: 1090,
    totalTimeLabel: '1.09 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(11, 2),
  }),
  probeRecord({
    id: 'probe-007',
    startedAt: '2026-07-30T14:06:39+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4',
    modelName: 'gpt-5.4',
    firstTokenMs: 405,
    firstTokenLabel: '405 ms',
    totalTimeMs: 1790,
    totalTimeLabel: '1.79 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(12, 2),
  }),
  probeRecord({
    id: 'probe-008',
    startedAt: '2026-07-30T14:05:57+08:00',
    status: 'error',
    siteId: 'gemini-asia',
    siteName: 'Gemini 亚洲线路',
    modelId: 'gemini-pro',
    modelName: 'gemini-2.5-pro',
    firstTokenMs: null,
    firstTokenLabel: '—',
    totalTimeMs: 612,
    totalTimeLabel: '612 ms',
    result: 'HTTP 429 · 请求受限',
    endpoint: 'https://gemini-gateway.example/v1/chat/completions',
    httpStatus: 429,
    responseMode: 'synchronous',
    errorDetail: '站点返回 rate_limit_exceeded，本次探测不会立即重试。',
    tokenUsage: noTokenUsage,
  }),
  probeRecord({
    id: 'probe-009',
    startedAt: '2026-07-30T14:04:42+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4-mini',
    modelName: 'gpt-5.4-mini',
    firstTokenMs: 291,
    firstTokenLabel: '291 ms',
    totalTimeMs: 1180,
    totalTimeLabel: '1.18 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(15, 2),
  }),
  probeRecord({
    id: 'probe-010',
    startedAt: '2026-07-30T14:02:42+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4-mini',
    modelName: 'gpt-5.4-mini',
    firstTokenMs: 274,
    firstTokenLabel: '274 ms',
    totalTimeMs: 1110,
    totalTimeLabel: '1.11 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(11, 2),
  }),
  probeRecord({
    id: 'probe-011',
    startedAt: '2026-07-30T14:00:39+08:00',
    status: 'healthy',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'gpt-5-4',
    modelName: 'gpt-5.4',
    firstTokenMs: 438,
    firstTokenLabel: '438 ms',
    totalTimeMs: 1880,
    totalTimeLabel: '1.88 s',
    result: '响应正常',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    tokenUsage: reportedTokenUsage(12, 2),
  }),
  probeRecord({
    id: 'probe-012',
    startedAt: '2026-07-29T12:58:43+08:00',
    status: 'warning',
    siteId: 'openai-main',
    siteName: 'OpenAI 主线路',
    modelId: 'o3-mini',
    modelName: 'o3-mini',
    firstTokenMs: 722,
    firstTokenLabel: '722 ms',
    totalTimeMs: 2490,
    totalTimeLabel: '2.49 s',
    result: '响应偏慢',
    endpoint: 'https://api.openai.com/v1/chat/completions',
    probeMode: '模型覆盖 · 最低推理',
    tokenUsage: reportedTokenUsage(13, 2),
  }),
  probeRecord({
    id: 'probe-013',
    startedAt: '2026-07-27T13:56:58+08:00',
    status: 'healthy',
    siteId: 'claude-backup',
    siteName: 'Claude 备用线路',
    modelId: 'claude-sonnet',
    modelName: 'claude-3-7-sonnet',
    firstTokenMs: 512,
    firstTokenLabel: '512 ms',
    totalTimeMs: 1970,
    totalTimeLabel: '1.97 s',
    result: '响应正常 · 未上报用量',
    endpoint: 'https://claude-gateway.example/v1/chat/completions',
    tokenUsage: noTokenUsage,
  }),
  probeRecord({
    id: 'probe-014',
    startedAt: '2026-07-18T13:54:41+08:00',
    status: 'healthy',
    siteId: 'gemini-asia',
    siteName: 'Gemini 亚洲线路',
    modelId: 'gemini-pro',
    modelName: 'gemini-2.5-pro',
    firstTokenMs: 980,
    firstTokenLabel: '980 ms',
    totalTimeMs: 3840,
    totalTimeLabel: '3.84 s',
    result: '响应正常',
    endpoint: 'https://gemini-gateway.example/v1/chat/completions',
    responseMode: 'synchronous',
    tokenUsage: reportedTokenUsage(14, 2),
  }),
];

export function createFixtureSnapshot(): AppSnapshot {
  return structuredClone({
    revision: 0,
    sites,
    selectedSiteId: 'openai-main',
    generalSettings: {
      launchAtStartup: true,
      continueInBackground: true,
      trayAction: 'panel',
      theme: 'system',
      language: 'zh-CN',
      historyRetentionDays: 30,
      diagnosticCaptureEnabled: true,
    },
    probeRecords,
    tray: {
      healthy: {
        status: 'healthy',
        activeSites: 3,
        activeModels: 13,
        alertCount: 0,
        alerts: [],
        completedLabel: '刚刚完成探测',
        nextProbeLabel: '下一次 1 分钟后',
      },
      alerts: {
        status: 'error',
        activeSites: 3,
        activeModels: 13,
        alertCount: 5,
        alerts: [
          {
            id: 'claude-alert',
            siteName: 'Claude 备用线路',
            modelName: 'claude-3-7-sonnet',
            status: 'error',
            firstToken: '超时',
            totalTime: '8.00 s',
          },
          {
            id: 'gemini-alert',
            siteName: 'Gemini 亚洲线路',
            modelName: 'gemini-2.5-pro',
            status: 'warning',
            firstToken: '1.82 s',
            totalTime: '6.41 s',
          },
          {
            id: 'openai-alert',
            siteName: 'OpenAI 主线路',
            modelName: 'o3-mini',
            status: 'error',
            firstToken: '693 ms',
            totalTime: '失败',
          },
        ],
        completedLabel: '刚刚完成探测',
        nextProbeLabel: '下一次 1 分钟后',
      },
    },
  } satisfies AppSnapshot);
}
