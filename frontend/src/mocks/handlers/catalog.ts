/**
 * 公共配置 catalog 的浏览器 mock：通知标题标记、模板变量与站点协议目录。
 * 静态数据（不入库），联调用 fixture，可直接改这里。
 */
import type { NotificationMarker, NotificationVariable } from '@/types/domain/notifications';
import type { SiteProtocolOption } from '@/types/domain/sites';

import { registerMockHandler } from './registry';

const siteProtocols: SiteProtocolOption[] = [
  { value: 'openai-compatible', label: 'OpenAI 兼容协议' },
  { value: 'openai-responses', label: 'OpenAI Responses' },
  { value: 'anthropic', label: 'Anthropic 协议' },
  { value: 'custom-http', label: '自定义 HTTP' },
];

registerMockHandler('get_site_protocols', () => structuredClone(siteProtocols));

const markers: NotificationMarker[] = [
  { markerId: 'none', symbol: '', name: '无标记', tone: 'neutral' },
  { markerId: 'info', symbol: 'ℹ️', name: '信息', tone: 'info' },
  { markerId: 'success', symbol: '✅', name: '成功', tone: 'positive' },
  { markerId: 'warning', symbol: '⚠️', name: '警告', tone: 'warning' },
  { markerId: 'error', symbol: '❌', name: '错误', tone: 'critical' },
  { markerId: 'bell', symbol: '🔔', name: '提醒', tone: 'info' },
  { markerId: 'fire', symbol: '🔥', name: '严重', tone: 'critical' },
];

const variables: NotificationVariable[] = [
  {
    key: 'time',
    label: '时间',
    description: '事件发生时间',
    example: '2026-10-11 14:30',
    group: '通用变量',
  },
  {
    key: 'app',
    label: '应用名',
    description: '应用名称',
    example: 'RelayPulse',
    group: '通用变量',
  },
  { key: 'model', label: '模型', description: '相关模型名', example: 'gpt-4o', group: '事件变量' },
  {
    key: 'endpoint',
    label: '端点',
    description: '探测目标端点',
    example: 'api.openai.com',
    group: '事件变量',
  },
];

registerMockHandler('get_notification_markers', () => structuredClone(markers));

registerMockHandler('get_notification_variables', () => structuredClone(variables));
