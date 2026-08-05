import type {
  CustomProbeSettings,
  PassThroughParameter,
  ProbeSettings,
  SiteProtocol,
} from './types';

export const customReservedProbeFields = new Set(['model']);
const unsafePathSegments = new Set(['__proto__', 'prototype', 'constructor']);

export function reservedProbeFields(protocol: SiteProtocol) {
  if (protocol === 'openai-responses') return new Set(['model', 'input', 'stream']);
  if (protocol === 'openai-compatible' || protocol === 'anthropic') {
    return new Set(['model', 'messages', 'stream']);
  }
  return customReservedProbeFields;
}

function parameterPath(key: string) {
  return key.split('.').map((segment) => segment.trim());
}

export function isParameterPathValid(key: string) {
  const path = parameterPath(key);
  return (
    path.length > 0 &&
    path.every((segment) => segment.length > 0 && !unsafePathSegments.has(segment.toLowerCase()))
  );
}

export function isReservedParameter(key: string, reservedFields: Set<string>) {
  const [root] = parameterPath(key);
  return Boolean(root && reservedFields.has(root.toLowerCase()));
}

export function isParameterValueValid(parameter: PassThroughParameter) {
  if (parameter.type === 'number') return Number.isFinite(Number(parameter.value));
  if (parameter.type !== 'json') return true;
  try {
    JSON.parse(parameter.value);
    return true;
  } catch {
    return false;
  }
}

function parameterValue(parameter: PassThroughParameter): unknown {
  if (parameter.type === 'boolean') return parameter.value === 'true';
  if (parameter.type === 'number') return Number(parameter.value);
  if (parameter.type === 'json') return JSON.parse(parameter.value);
  return parameter.value;
}

function appendParameters(
  request: Record<string, unknown>,
  parameters: PassThroughParameter[],
  reservedFields: Set<string>,
) {
  for (const parameter of parameters) {
    const key = parameter.key.trim();
    if (
      !key ||
      !isParameterPathValid(key) ||
      isReservedParameter(key, reservedFields) ||
      !isParameterValueValid(parameter)
    ) {
      continue;
    }
    setParameterPath(request, key, parameterValue(parameter));
  }
}

function setParameterPath(request: Record<string, unknown>, key: string, value: unknown) {
  const path = parameterPath(key);
  let current = request;

  for (const segment of path.slice(0, -1)) {
    const existing = current[segment];
    if (!existing || typeof existing !== 'object' || Array.isArray(existing)) {
      current[segment] = {};
    }
    current = current[segment] as Record<string, unknown>;
  }

  current[path[path.length - 1]] = value;
}

export function buildProbeRequest(
  modelName: string,
  protocol: SiteProtocol,
  settings: ProbeSettings,
) {
  const request: Record<string, unknown> = { model: modelName };
  if (protocol === 'openai-responses') {
    Object.assign(request, {
      input: settings.prompt,
      stream: true,
      max_output_tokens: settings.maxOutputTokens,
      reasoning: { effort: 'low' },
    });
  } else if (protocol === 'openai-compatible') {
    Object.assign(request, {
      messages: [{ role: 'user', content: settings.prompt }],
      stream: true,
      max_tokens: settings.maxOutputTokens,
      reasoning_effort: 'low',
    });
  } else if (protocol === 'anthropic') {
    Object.assign(request, {
      messages: [{ role: 'user', content: settings.prompt }],
      stream: true,
      max_tokens: settings.maxOutputTokens,
    });
  }

  appendParameters(request, settings.parameters, reservedProbeFields(protocol));

  return request;
}

export function buildCustomProbeRequest(modelName: string, settings: CustomProbeSettings) {
  const request: Record<string, unknown> = { model: modelName };
  appendParameters(request, settings.parameters, customReservedProbeFields);
  return request;
}

export function resolveCustomProbeEndpoint(baseUrl: string, endpoint: string) {
  const trimmed = endpoint.trim();
  const base = new URL(baseUrl);
  const resolved = trimmed.startsWith('/')
    ? new URL(trimmed, base.origin)
    : new URL(trimmed, `${baseUrl.replace(/\/+$/, '')}/`);
  if (resolved.origin !== base.origin) {
    throw new Error('Custom probe endpoint must use the site origin');
  }
  return resolved.toString();
}
