import type { RequestParameter, SiteProtocol } from '@/types/domain/sites';

import { requestHeaderValidation, requestPreview } from '@/utils/request-headers';

/** 站点凭据由专用字段管理；此规则不属于通用 HTTP 字段语法。 */
export function credentialHeaderError(name: string): string | null {
  return ['authorization', 'x-api-key', 'cookie', 'proxy-authorization'].includes(
    name.toLowerCase(),
  )
    ? '访问凭据请使用专用凭据字段'
    : null;
}

export function siteHeaderValidation(parameters: RequestParameter[]) {
  return requestHeaderValidation(parameters).map((validation, index) => ({
    ...validation,
    key:
      validation.key ??
      (parameters[index].enabled ? credentialHeaderError(parameters[index].key) : null),
  }));
}

export function siteHeadersError(parameters: RequestParameter[]) {
  return (
    siteHeaderValidation(parameters)
      .flatMap((item) => [item.key, item.value])
      .find(Boolean) ?? null
  );
}

export function sensitiveSiteHeader(name: string): boolean {
  const key = name.toLowerCase().replace(/[-_]/gu, '');
  return (
    /authorization|key|token|secret|password|credential|cookie/iu.test(key) ||
    ['cookie', 'setcookie', 'xauth'].includes(key)
  );
}

/** 站点协议与允许的默认头只参与预览，不修改配置或执行请求。 */
export function siteRequestPreview(
  address: string,
  protocol: SiteProtocol,
  parameters: RequestParameter[],
  body: Record<string, unknown>,
) {
  const error = siteHeadersError(parameters);
  if (error) throw new Error(error);
  const defaults: Record<string, string> = {
    ...(protocol === 'anthropic'
      ? { 'x-api-key': '[已配置凭据]' }
      : { authorization: '[REDACTED]' }),
    'content-type': 'application/json',
    accept: '*/*',
    'user-agent': '[应用默认 User-Agent]',
  };
  return requestPreview(address, parameters, body, defaults, sensitiveSiteHeader);
}
