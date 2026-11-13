import type { RequestHeader, RequestParameter } from '@/types/domain/sites';
import type { RequestParameterDraft, RequestPreview } from '@/types/request-parameters';

import { httpHeaderNameError, httpHeaderValueError } from './http-headers';
import { newRequestParameter } from './request-parameters';

export function requestHeaderValidation(parameters: RequestParameter[]) {
  const active = parameters.filter((p) => p.enabled);
  return parameters.map((p) => {
    if (!p.enabled) return { key: null, value: null };
    let key = p.key === '' ? '请输入请求头名称' : httpHeaderNameError(p.key);
    if (
      !key &&
      active.some((other) => other !== p && other.key.toLowerCase() === p.key.toLowerCase())
    )
      key = '请求头不能重复';
    const value =
      p.type !== 'string' || p.binding
        ? '请求头须使用无动态绑定的 string 类型'
        : httpHeaderValueError(p.value);
    return { key, value };
  });
}

export function requestHeadersError(parameters: RequestParameter[]) {
  return (
    requestHeaderValidation(parameters)
      .flatMap((item) => [item.key, item.value])
      .find(Boolean) ?? null
  );
}

export function buildRequestHeaders(parameters: RequestParameter[]): RequestHeader[] {
  const error = requestHeadersError(parameters);
  if (error) throw new Error(error);
  return parameters.filter((p) => p.enabled).map(({ key, value }) => ({ key, value }));
}

export function mergeRequestHeadersFromJson(parameters: RequestParameterDraft[], text: string) {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error('JSON 格式错误，请粘贴有效的 JSON 对象');
  }
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error('参数须为 JSON 对象');
  const imported = Object.entries(value).map(([key, value]) => {
    if (typeof value !== 'string') throw new Error('请求头的值须为字符串，请修改 JSON 后导入');
    return { ...newRequestParameter(key), value };
  });
  const result = parameters.map((p) => ({ ...p }));
  for (const p of imported) {
    const index = result.findIndex(
      (existing) => existing.key.toLowerCase() === p.key.toLowerCase(),
    );
    if (index < 0) result.push(p);
    else result[index] = { ...p, id: result[index].id };
  }
  // 同一次导入的大小写重复必须报错，不能按顺序静默覆盖。
  const error = requestHeadersError(imported) ?? requestHeadersError(result);
  if (error) throw new Error(error);
  return result;
}

/** 只组合调用方提供的头；不识别站点协议或凭据规则。 */
export function requestPreview(
  address: string,
  parameters: RequestParameter[],
  body: Record<string, unknown>,
  defaultHeaders: Record<string, string>,
  sensitiveHeader: (name: string) => boolean,
): RequestPreview {
  const configured = Object.fromEntries(
    buildRequestHeaders(parameters).map(({ key, value }) => [key.toLowerCase(), value]),
  );
  const defaults = Object.fromEntries(
    Object.entries(defaultHeaders).filter(
      ([name]) => !Object.prototype.hasOwnProperty.call(configured, name.toLowerCase()),
    ),
  );
  return {
    method: 'POST',
    address,
    headers: {
      configured: Object.fromEntries(
        Object.entries(configured).map(([name, value]) => [
          name,
          sensitiveHeader(name) ? '[REDACTED]' : value,
        ]),
      ),
      defaults,
    },
    body,
  };
}
