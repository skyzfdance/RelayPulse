import type { CustomProbeSettings, PassThroughParameter, ProbeSettings } from './types';

export const reservedProbeFields = new Set(['model', 'messages', 'stream']);
export const customReservedProbeFields = new Set(['model']);

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
    if (!key || reservedFields.has(key) || !isParameterValueValid(parameter)) continue;
    request[key] = parameterValue(parameter);
  }
}

export function buildProbeRequest(modelName: string, settings: ProbeSettings) {
  const request: Record<string, unknown> = {
    model: modelName,
    messages: [{ role: 'user', content: settings.prompt }],
    stream: true,
    max_output_tokens: settings.maxOutputTokens,
  };

  appendParameters(request, settings.parameters, reservedProbeFields);

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
