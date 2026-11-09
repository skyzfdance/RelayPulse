import type { ParameterType, RequestParameter } from '@/types/domain/sites';
import type { RequestParameterDraft } from '@/types/request-parameters';

let nextId = 0;

export function newRequestParameter(
  key = '',
  type: ParameterType = 'string',
): RequestParameterDraft {
  return {
    id: `request-parameter-${++nextId}`,
    key,
    value: type === 'object' ? '{}' : '',
    type,
    enabled: true,
  };
}

export function createRequestParameterDrafts(
  parameters: RequestParameter[],
): RequestParameterDraft[] {
  return parameters.map((parameter) => ({ ...newRequestParameter(), ...parameter }));
}

export function serializeRequestParameters(
  parameters: RequestParameterDraft[],
): RequestParameter[] {
  return parameters.map(({ key, value, type, enabled, binding }) => ({
    key,
    value,
    type,
    enabled,
    ...(binding ? { binding } : {}),
  }));
}

/** 创建配置时将基础请求转换为参数表，保存和发送时不补充字段。 */
export function createRequestParameters(body: Record<string, unknown>): RequestParameterDraft[] {
  const parameters: RequestParameterDraft[] = [];
  function append(value: unknown, key: string) {
    const parameter = newRequestParameter(key);
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      parameter.type = 'object';
      parameter.value = '{}';
      parameters.push(parameter);
      for (const [child, childValue] of Object.entries(value))
        append(childValue, `${key}.${child}`);
      return;
    }
    if (typeof value === 'number' || typeof value === 'boolean') {
      parameter.type = typeof value as 'number' | 'boolean';
      parameter.value = String(value);
    } else if (typeof value === 'string') parameter.value = value;
    else {
      parameter.type = 'json';
      parameter.value = JSON.stringify(value);
    }
    if (key === 'model') parameter.binding = 'model';
    parameters.push(parameter);
  }
  for (const [key, value] of Object.entries(body)) append(value, key);
  return parameters;
}

/** 粘贴内容覆盖相同路径；未提及的参数和草稿状态保持不变。 */
export function mergeRequestParametersFromJson(
  parameters: RequestParameterDraft[],
  json: string,
): RequestParameterDraft[] {
  let body: unknown;
  try {
    body = JSON.parse(json);
  } catch {
    throw new Error('JSON 格式错误，请粘贴有效的 JSON 对象');
  }
  if (body === null || typeof body !== 'object' || Array.isArray(body))
    throw new Error('参数须为 JSON 对象，不能是数组、null 或单个值');

  function validateValue(value: unknown) {
    if (typeof value === 'number' && !Number.isFinite(value))
      throw new Error('JSON 中的数字必须是有限数值');
    if (value === null || typeof value !== 'object') return;
    for (const [key, child] of Object.entries(value)) {
      if (
        key
          .split('.')
          .some((part) => ['__proto__', 'prototype', 'constructor'].includes(part.toLowerCase()))
      )
        throw new Error('JSON 中包含不安全的对象字段');
      validateValue(child);
    }
  }
  validateValue(body);

  const imported = createRequestParameters(body as Record<string, unknown>);
  for (const [index, parameter] of imported.entries()) {
    const keyError = parameterKeyError(imported, index);
    if (keyError) throw new Error(keyError);
    const children = imported.some((child) => child.key.startsWith(`${parameter.key}.`));
    const binding = parameters.find(
      (existing) => existing.key === parameter.key && existing.binding,
    );
    if (binding) {
      if (children) throw new Error(`动态模型参数 ${parameter.key} 不能包含子参数`);
      parameter.binding = binding.binding;
      parameter.type = 'string';
      parameter.value = binding.value;
    }
    if (children && parameter.type !== 'object')
      throw new Error(`参数路径冲突：${parameter.key} 同时包含值和子参数`);
    const valueError = parameterValueError(parameter);
    if (valueError) throw new Error(valueError);
  }

  const result = parameters.map((parameter) => ({ ...parameter }));
  function upsert(parameter: RequestParameterDraft, replaceChildren = false) {
    const matches = result.filter((existing) => existing.key === parameter.key);
    const previous = matches.find((existing) => existing.binding) ?? matches[0];
    if (previous?.binding && parameter.type === 'object')
      throw new Error(`动态模型参数 ${parameter.key} 不能包含子参数`);
    // JSON 对象同样参与叶路径合并；仅在需要保留对象子字段时展开。
    if (previous?.type === 'json' && parameter.type === 'object' && !replaceChildren) {
      let value: unknown;
      try {
        value = JSON.parse(previous.value);
        validateValue(value);
      } catch {
        // 无效 JSON 按被覆盖的普通值处理。
        value = undefined;
      }
      if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
        const expanded = createRequestParameters({ [previous.key]: value });
        const valid = requestParameterValidation(expanded).every(
          (item) => !item.key && !item.value,
        );
        if (valid) {
          for (const child of expanded.slice(1))
            if (!result.some((existing) => existing.key === child.key)) result.push(child);
        }
      }
    }
    const replacement = { ...parameter, id: previous?.id ?? parameter.id, enabled: true };
    if (previous?.binding) {
      replacement.binding = previous.binding;
      replacement.type = 'string';
      replacement.value = previous.value;
    }
    let insertion = result.length;
    for (let index = result.length - 1; index >= 0; index--) {
      const existing = result[index];
      if (
        existing.key === parameter.key ||
        (replaceChildren && existing.key.startsWith(`${parameter.key}.`))
      ) {
        insertion = index;
        result.splice(index, 1);
      }
    }
    result.splice(insertion, 0, replacement);
  }

  const ordered = imported.sort((a, b) => a.key.split('.').length - b.key.split('.').length);
  for (const parameter of ordered) {
    const path = parameter.key.split('.');
    for (let depth = 1; depth < path.length; depth++)
      upsert(newRequestParameter(path.slice(0, depth).join('.'), 'object'));
    const hasImportedChildren = imported.some((child) => child.key.startsWith(`${parameter.key}.`));
    upsert(parameter, parameter.type !== 'object' || !hasImportedChildren);
  }
  return result;
}

function ancestors<T extends RequestParameter>(parameters: T[], parameter: T) {
  return parameters.filter((parent) => parent.key && parameter.key.startsWith(`${parent.key}.`));
}

export function parameterBlocked(parameters: RequestParameter[], parameter: RequestParameter) {
  return ancestors(parameters, parameter).some((parent) => !parent.enabled);
}

export function parameterEnabled(parameters: RequestParameter[], parameter: RequestParameter) {
  return parameter.enabled && !parameterBlocked(parameters, parameter);
}

export function requestParameterRows(parameters: RequestParameterDraft[]) {
  const children = new Map<string | undefined, RequestParameterDraft[]>();
  for (const parameter of parameters) {
    const parent = ancestors(parameters, parameter).sort((a, b) => b.key.length - a.key.length)[0];
    const siblings = children.get(parent?.id) ?? [];
    siblings.push(parameter);
    children.set(parent?.id, siblings);
  }
  const rows: { parameter: RequestParameterDraft; depth: number; blocked: boolean }[] = [];
  function append(parent: string | undefined, depth: number) {
    for (const parameter of children.get(parent) ?? []) {
      rows.push({ parameter, depth, blocked: parameterBlocked(parameters, parameter) });
      append(parameter.id, depth + 1);
    }
  }
  append(undefined, 0);
  return rows;
}

export function updateRequestParameterKey(
  parameters: RequestParameterDraft[],
  parameter: RequestParameterDraft,
  key: string,
) {
  const previous = parameter.key;
  parameter.key = key;
  if (previous !== key) {
    for (const child of parameters) {
      if (child !== parameter && child.key.startsWith(`${previous}.`))
        child.key = `${key}${child.key.slice(previous.length)}`;
    }
  }
  const path = key.split('.');
  if (path.some((part) => !part || part !== part.trim())) return;
  for (let depth = 1; depth < path.length; depth++) {
    const parent = path.slice(0, depth).join('.');
    if (!parameters.some((item) => item.key === parent))
      parameters.splice(parameters.indexOf(parameter), 0, newRequestParameter(parent, 'object'));
  }
}

export function removeRequestParameter(
  parameters: RequestParameterDraft[],
  parameter: RequestParameterDraft,
) {
  for (let index = parameters.length - 1; index >= 0; index--) {
    const candidate = parameters[index];
    if (
      candidate.id === parameter.id ||
      (parameter.key && candidate.key.startsWith(`${parameter.key}.`))
    )
      parameters.splice(index, 1);
  }
}

export function parameterKeyError(parameters: RequestParameter[], index: number): string | null {
  const key = parameters[index].key;
  if (!key.trim()) return '请输入参数字段';
  if (
    key
      .split('.')
      .some(
        (part) =>
          !part ||
          part !== part.trim() ||
          ['__proto__', 'prototype', 'constructor'].includes(part.toLowerCase()),
      )
  )
    return '参数路径无效，请使用 reasoning.effort 这类对象路径';
  if (parameters.some((parameter, other) => other !== index && parameter.key === key))
    return '参数字段不能重复';
  return null;
}

export function parameterValueError(parameter: RequestParameter): string | null {
  if (parameter.binding)
    return parameter.type === 'string' ? null : '动态模型参数须使用 string 类型';
  if (parameter.type === 'string' || parameter.type === 'object') return null;
  if (parameter.type === 'number')
    return parameter.value.trim() && Number.isFinite(Number(parameter.value))
      ? null
      : '数字参数必须为有效数字';
  if (parameter.type === 'boolean')
    return ['true', 'false'].includes(parameter.value) ? null : '布尔参数只能填写 true 或 false';
  if (parameter.type === 'json') {
    try {
      JSON.parse(parameter.value);
      return null;
    } catch {
      return 'JSON 参数格式错误';
    }
  }
  return '请选择有效的参数类型';
}

export function requestParameterValidation(parameters: RequestParameter[]) {
  const active = parameters.filter((parameter) => parameterEnabled(parameters, parameter));
  return parameters.map((parameter) => {
    const index = active.indexOf(parameter);
    if (index === -1) return { key: null, value: null };
    const parentError = ancestors(parameters, parameter).some((parent) => parent.type !== 'object')
      ? '有子参数的父级须使用 object 类型'
      : null;
    return {
      key: parameterKeyError(active, index) ?? parentError,
      value: parameterValueError(parameter),
    };
  });
}

export function buildRequestParametersPreview(
  parameters: RequestParameter[],
  modelName: string,
): Record<string, unknown> {
  const error = requestParameterValidation(parameters)
    .flatMap((item) => [item.key, item.value])
    .find(Boolean);
  if (error) throw new Error(error);
  const body: Record<string, unknown> = {};
  const active = parameters
    .filter((parameter) => parameterEnabled(parameters, parameter))
    .sort((a, b) => a.key.split('.').length - b.key.split('.').length);
  for (const parameter of active) {
    const path = parameter.key.split('.');
    let target = body;
    for (const part of path.slice(0, -1)) {
      if (!Object.prototype.hasOwnProperty.call(target, part)) target[part] = {};
      target = target[part] as Record<string, unknown>;
    }
    let value: unknown = parameter.value;
    if (parameter.binding === 'model') value = modelName;
    else if (parameter.type === 'object') value = {};
    else if (parameter.type === 'number') value = Number(parameter.value);
    else if (parameter.type === 'boolean') value = parameter.value === 'true';
    else if (parameter.type === 'json') value = JSON.parse(parameter.value);
    target[path[path.length - 1]] = value;
  }
  return body;
}
