import type { CustomProbeRequest, ProbePolicy, RequestParameter } from '@/types/domain/sites';

import { requestParameterValidation } from '@/utils/request-parameters';
import { requestUrl } from '@/utils/request-url';

import { siteHeadersError } from './site-request';

export function parametersError(parameters: RequestParameter[]): string | null {
  return (
    requestParameterValidation(parameters)
      .flatMap((item) => [item.key, item.value])
      .find(Boolean) ?? null
  );
}

export function policyError(policy: ProbePolicy): string | null {
  return policy.version === 2
    ? (siteHeadersError(policy.headers) ?? parametersError(policy.parameters))
    : '请求参数配置版本无效';
}

export function resolveCustomProbeEndpoint(baseUrl: string, endpoint: string): string {
  if (!endpoint.trim()) throw new Error('请输入自定义探测端点');
  if (endpoint !== endpoint.trim()) throw new Error('探测端点首尾不能有空格');
  if (/[\r\n\t]/u.test(endpoint)) throw new Error('请输入有效的探测端点');
  return requestUrl(baseUrl, endpoint, true);
}

export function customEndpointError(baseUrl: string, endpoint: string): string | null {
  try {
    resolveCustomProbeEndpoint(baseUrl, endpoint);
    return null;
  } catch (cause) {
    return cause instanceof TypeError ? '请输入有效的探测端点' : (cause as Error).message;
  }
}

export function customProbeError(request: CustomProbeRequest, baseUrl: string): string | null {
  return customEndpointError(baseUrl, request.endpoint) ?? policyError(request);
}
