export function formatIntelligenceModelName(model: string) {
  const match = /^gpt-(\d+(?:\.\d+)*)(?:-([a-z][a-z0-9-]*))?$/i.exec(model);
  if (!match) return model;

  const [, version, suffix] = match;
  const displaySuffix = suffix
    ?.split('-')
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1).toLowerCase()}`)
    .join(' ');
  return `GPT-${version}${displaySuffix ? ` ${displaySuffix}` : ''}`;
}

export function formatIntelligenceEffort(effort: string) {
  return effort.toUpperCase();
}

export function formatIntelligenceTableEffort(effort: string) {
  return effort.toLowerCase();
}

export function formatIntelligenceIq(value: number) {
  return new Intl.NumberFormat('zh-CN', {
    minimumFractionDigits: 1,
    maximumFractionDigits: 2,
  }).format(value);
}
