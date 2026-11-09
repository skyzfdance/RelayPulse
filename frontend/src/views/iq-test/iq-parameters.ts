import type { RequestParameter } from '@/types/domain/sites';

import {
  buildRequestParametersPreview,
  parameterEnabled,
  requestParameterValidation,
} from '@/utils/request-parameters';

export function iqPresets(protocol: string) {
  if (protocol === 'anthropic')
    return [
      { value: 'off', label: '关闭' },
      { value: 'low', label: '低' },
      { value: 'medium', label: '中' },
      { value: 'high', label: '高' },
      { value: 'custom', label: '自定义参数…' },
    ];
  if (protocol === 'custom-http') return [{ value: 'custom', label: '自定义参数…' }];
  return [
    { value: 'minimal', label: '最低 (minimal)' },
    { value: 'low', label: '低 (low)' },
    { value: 'medium', label: '中 (medium)' },
    { value: 'high', label: '高 (high)' },
    { value: 'xhigh', label: '超高 (xhigh)' },
    { value: 'max', label: '最大 (max)' },
    { value: 'custom', label: '自定义参数…' },
  ];
}

const reserved = new Set(['model', 'messages', 'prompt', 'input', 'stream', 'system']);
function unsafeJsonKey(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false;
  return Object.entries(value).some(
    ([key, child]) =>
      ['__proto__', 'prototype', 'constructor'].includes(key.toLowerCase()) || unsafeJsonKey(child),
  );
}

export function iqParameterError(parameters: RequestParameter[]): string | undefined {
  if (
    parameters.some(
      (parameter) =>
        parameterEnabled(parameters, parameter) && reserved.has(parameter.key.split('.')[0]),
    )
  )
    return '题面、model、stream 等系统字段不可覆盖';
  for (const parameter of parameters) {
    if (!parameterEnabled(parameters, parameter) || parameter.type !== 'json') continue;
    try {
      if (unsafeJsonKey(JSON.parse(parameter.value))) return 'JSON 中包含不安全的对象字段';
    } catch {
      /* JSON 格式错误由参数校验返回。 */
    }
  }
  return requestParameterValidation(parameters)
    .flatMap((item) => [item.key, item.value])
    .find((error): error is string => Boolean(error));
}

function mergeParameters(base: Record<string, unknown>, overrides: Record<string, unknown>) {
  for (const [key, value] of Object.entries(overrides)) {
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      const previous = Object.prototype.hasOwnProperty.call(base, key) ? base[key] : undefined;
      base[key] = mergeParameters(
        previous && typeof previous === 'object' && !Array.isArray(previous)
          ? (previous as Record<string, unknown>)
          : {},
        value as Record<string, unknown>,
      );
    } else base[key] = value;
  }
  return base;
}

export function buildIqParameters(
  protocol: string,
  preset: string,
  parameters: RequestParameter[],
): Record<string, unknown> {
  const error = iqParameterError(parameters);
  if (error) throw new Error(error);
  let body: Record<string, unknown> = {};
  if (preset !== 'custom') {
    if (protocol === 'anthropic') {
      body = {
        thinking:
          preset === 'off'
            ? { type: 'disabled' }
            : { type: 'enabled', budget_tokens: { low: 1024, medium: 8192, high: 32000 }[preset] },
      };
    } else if (protocol === 'openai-responses') body = { reasoning: { effort: preset } };
    else if (protocol !== 'custom-http') body = { reasoning_effort: preset };
  }
  return mergeParameters(body, buildRequestParametersPreview(parameters, ''));
}

export function iqStartError(
  questionIds: string[],
  participants: { modelName: string; precheck: { status: string } }[],
  timeout: number | null,
  retries: number | null,
) {
  if (!questionIds.length) return '请先选择题目';
  if (!participants.length) return '请添加参与者';
  const blocked = participants.find((p) => p.precheck.status !== 'available');
  if (blocked) return `${blocked.modelName} 预检未通过`;
  if (timeout === null || !Number.isInteger(timeout) || timeout < 60 || timeout > 300)
    return '超时须为 60–300 秒整数';
  if (retries === null || !Number.isInteger(retries) || retries < 5 || retries > 20)
    return '重试须为 5–20 次整数';
  return undefined;
}
