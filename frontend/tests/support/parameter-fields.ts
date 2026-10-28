import type { ProbePolicy } from '@/types/domain/sites';

import {
  buildRequestParametersPreview,
  createRequestParameters,
  serializeRequestParameters,
} from '@/views/config/sites/request-parameters';

export const parameterKeySelector = (key: string) =>
  `[data-parameter-key="${key}"] [aria-label^="参数字段 "]`;

export const parameterValueSelector = (key: string) =>
  `[data-parameter-key="${key}"] [aria-label^="参数值 "]`;

export const messagesValue = (content: string) => JSON.stringify([{ role: 'user', content }]);

export function policyWithBody(body: Record<string, unknown>): ProbePolicy {
  return { version: 2, parameters: serializeRequestParameters(createRequestParameters(body)) };
}

export function chatPolicy(
  prompt = '只回复 OK',
  maxTokens = 8,
  extra: Record<string, unknown> = {},
) {
  return policyWithBody({
    model: '',
    messages: [{ role: 'user', content: prompt }],
    stream: true,
    max_tokens: maxTokens,
    reasoning_effort: 'low',
    ...extra,
  });
}

export function policyBody(policy: ProbePolicy) {
  return buildRequestParametersPreview(policy.parameters, '<模型名称>');
}

export function policyPrompt(policy: ProbePolicy | null | undefined): string | undefined {
  if (!policy) return undefined;
  const body = policyBody(policy);
  return typeof body.input === 'string'
    ? body.input
    : (body.messages as { content: string }[] | undefined)?.[0]?.content;
}

export function setPolicyPrompt(policy: ProbePolicy, value: string) {
  const parameter = policy.parameters.find(
    (parameter) => parameter.key === 'input' || parameter.key === 'messages',
  )!;
  parameter.value = parameter.key === 'input' ? value : messagesValue(value);
}
