import type { CustomProbeRequest, ProbePolicy, SiteProtocol } from '@/types/domain/sites';

import { customProbeError, policyError, resolveCustomProbeEndpoint } from './probe-policy';
import {
  buildRequestParametersPreview,
  createRequestParameters,
  serializeRequestParameters,
} from './request-parameters';

/** 只在创建配置时提供基础参数，之后由使用者维护整张参数表。 */
export function defaultProbePolicy(protocol: SiteProtocol): ProbePolicy {
  const body: Record<string, unknown> = { model: '' };
  if (protocol === 'openai-responses')
    Object.assign(body, {
      input: '只回复 OK',
      stream: true,
      max_output_tokens: 8,
      reasoning: { effort: 'low' },
    });
  else if (protocol === 'openai-compatible' || protocol === 'anthropic')
    Object.assign(body, {
      messages: [{ role: 'user', content: '只回复 OK' }],
      stream: true,
      max_tokens: 8,
      ...(protocol === 'openai-compatible' ? { reasoning_effort: 'low' } : {}),
    });
  return { version: 2, parameters: serializeRequestParameters(createRequestParameters(body)) };
}

export function defaultCustomProbeRequest(): CustomProbeRequest {
  return { version: 2, endpoint: '', parameters: defaultProbePolicy('custom-http').parameters };
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
