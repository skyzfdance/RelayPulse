import type { CustomProbeRequest, ProbePolicy, RequestParameter } from '@/types/domain/sites';

import { requestParameterValidation } from './request-parameters';

export function parametersError(parameters: RequestParameter[]): string | null {
  return (
    requestParameterValidation(parameters)
      .flatMap((item) => [item.key, item.value])
      .find(Boolean) ?? null
  );
}

export function policyError(policy: ProbePolicy): string | null {
  return policy.version === 2 ? parametersError(policy.parameters) : '请求参数配置版本无效';
}

export function resolveCustomProbeEndpoint(baseUrl: string, endpoint: string): string {
  if (!endpoint.trim()) throw new Error('请输入自定义探测端点');
  if (endpoint !== endpoint.trim()) throw new Error('探测端点首尾不能有空格');
  if (/[\r\n\t]/u.test(endpoint)) throw new Error('请输入有效的探测端点');
  const base = new URL(baseUrl);
  const resolved = endpoint.startsWith('/')
    ? new URL(endpoint, base.origin)
    : new URL(endpoint, `${baseUrl.replace(/\/+$/u, '')}/`);
  if (!['http:', 'https:'].includes(resolved.protocol) || resolved.origin !== base.origin)
    throw new Error('端点必须与站点 Base URL 同源');
  if (resolved.username || resolved.password) throw new Error('探测端点不能包含访问凭据');
  return resolved.toString();
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
