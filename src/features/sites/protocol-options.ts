import type { SiteProtocol } from '../../domain/types';

export const siteProtocolOptions: Array<{ value: SiteProtocol; label: string }> = [
  { value: 'openai-compatible', label: 'OpenAI 兼容协议' },
  { value: 'anthropic', label: 'Anthropic 协议' },
  { value: 'openai-responses', label: 'OpenAI Responses 协议' },
  { value: 'custom-http', label: '自定义 HTTP' },
];
