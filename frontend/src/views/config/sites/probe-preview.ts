import type { CustomProbeRequest, ProbePolicy, SiteProtocol } from '@/types/domain/sites';

import {
  buildRequestParametersPreview,
  createRequestParameters,
  serializeRequestParameters,
} from '@/utils/request-parameters';

import { customProbeError, policyError, resolveCustomProbeEndpoint } from './probe-policy';

const anthropicReasoningDefaults = {
  max_tokens: 4096,
  thinking: { type: 'adaptive' },
  output_config: { effort: 'low' },
};

/** 只在创建配置时提供基础参数，之后由使用者维护整张参数表。 */
export function defaultProbePolicy(protocol: SiteProtocol): ProbePolicy {
  const body: Record<string, unknown> = { model: '' };
  if (protocol === 'openai-responses')
    Object.assign(body, {
      input: '只回复 OK',
      stream: true,
      reasoning: { effort: 'low' },
    });
  else if (protocol === 'openai-compatible' || protocol === 'anthropic')
    Object.assign(body, {
      messages: [{ role: 'user', content: '只回复 OK' }],
      stream: true,
      ...(protocol === 'anthropic'
        ? anthropicReasoningDefaults
        : { max_tokens: 8, reasoning_effort: 'low' }),
    });
  return {
    version: 2,
    headers: [],
    parameters: serializeRequestParameters(createRequestParameters(body)),
  };
}

/** 按参数表生成实际请求，不根据协议补充字段。 */
export function buildProbePreview(modelName: string, _protocol: SiteProtocol, policy: ProbePolicy) {
  const error = policyError(policy);
  if (error) throw new Error(error);
  return buildRequestParametersPreview(policy.parameters, modelName);
}

export function buildCustomProbePreview(
  baseUrl: string,
  modelName: string,
  settings: CustomProbeRequest,
) {
  const error = customProbeError(settings, baseUrl);
  if (error) throw new Error(error);
  return {
    endpoint: resolveCustomProbeEndpoint(baseUrl, settings.endpoint),
    body: buildRequestParametersPreview(settings.parameters, modelName),
  };
}
