import type { Connection, RequestHeader } from '@/types/domain/sites';

export const connectionNumberFields = {
  intervalSeconds: {
    label: '探测频率',
    max: 86400,
    unit: '秒',
    tooltip: '每个已启用模型自动探测的间隔',
  },
  slowFirstTokenSeconds: {
    label: '首 Token 慢阈值',
    max: 100,
    unit: '秒',
    tooltip: '仅在流式响应提供首 Token 数据时参与黄灯判定；达到阈值不会中断请求或转为红灯',
  },
  slowTotalSeconds: {
    label: '总耗时慢阈值',
    max: 500,
    unit: '秒',
    tooltip: '请求成功但总耗时达到该值时显示黄灯；不会中断请求或转为红灯',
  },
  failureThreshold: {
    label: '连续失败转红',
    max: 10,
    unit: '次',
    tooltip: '连续失败达到该次数后显示红灯；此前保持上一次稳定状态，没有历史时保持灰色',
  },
  recoveryThreshold: {
    label: '连续成功恢复',
    max: 10,
    unit: '次',
    tooltip: '红灯后连续成功达到该次数才恢复；黄灯结果也计为成功',
  },
} as const;

export type ConnectionNumberField = keyof typeof connectionNumberFields;

function textError(value: unknown, label: string, optional = false): string | null {
  if (typeof value !== 'string' || (!optional && !value.trim())) return `请输入${label}`;
  if (value !== value.trim()) return `${label}首尾不能有空格`;
  return null;
}

export function endpointError(value: unknown, label: string, baseUrl: string): string | null {
  const error = textError(value, label);
  if (error) return error;
  if (/[\r\n]/u.test(value as string)) return `请输入有效的${label}`;
  try {
    const url = new URL(value as string, baseUrl);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password)
      return '请输入不含凭据的 HTTP 或 HTTPS 端点';
  } catch {
    return `请输入有效的${label}`;
  }
  return null;
}

/** raw 保留输入框原文，避免数字组件解析后把首尾空格或非数字输入当作有效值。 */
export function connectionNumberError(
  name: ConnectionNumberField,
  value: unknown,
  raw?: string,
): string | null {
  const { label, max, unit } = connectionNumberFields[name];
  if (raw !== undefined) {
    const error = textError(raw, label);
    if (error) return error;
  } else if (value === null || value === undefined || value === '') {
    return `请输入${label}`;
  }
  const number = raw === undefined ? value : Number(raw);
  if (
    (raw !== undefined && !/^\d+$/u.test(raw)) ||
    typeof number !== 'number' ||
    !Number.isInteger(number) ||
    number < 1 ||
    number > max
  )
    return `${label}须为 1–${max} 的整数（${unit}）`;
  return null;
}

export function headerNameError(
  value: unknown,
  headers: RequestHeader[],
  index: number,
): string | null {
  const error = textError(value, '请求头名称');
  if (error) return error;
  const key = (value as string).toLowerCase();
  if (!/^[!#$%&'*+.^_`|~0-9a-z-]+$/u.test(key)) return '请求头名称格式无效';
  if (['authorization', 'x-api-key', 'cookie', 'proxy-authorization'].includes(key))
    return '访问凭据请使用专用凭据字段';
  if (headers.some((header, i) => i !== index && header.key.toLowerCase() === key))
    return '请求头不能重复';
  return null;
}

export function headerValueError(value: unknown): string | null {
  const error = textError(value, '请求头值', true);
  if (error) return error;
  return /[\r\n]/u.test(value as string) ? '请求头值不能包含换行' : null;
}

export function validateConnection(connection: Connection, baseUrl: string): string | null {
  for (const [name, label] of [
    ['modelsPath', '模型列表端点'],
    ['probePath', '探测端点'],
  ] as const) {
    const error = endpointError(connection[name], label, baseUrl);
    if (error) return error;
  }
  for (const name of Object.keys(connectionNumberFields) as ConnectionNumberField[]) {
    const error = connectionNumberError(name, connection[name]);
    if (error) return error;
  }
  for (const [index, header] of connection.headers.entries()) {
    const error =
      headerNameError(header.key, connection.headers, index) ?? headerValueError(header.value);
    if (error) return error;
  }
  return null;
}
