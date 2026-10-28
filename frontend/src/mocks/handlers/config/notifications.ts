/**
 * 消息通知命令的浏览器 mock（config/notifications）。
 *
 * 维护进程内可变状态，使保存能真正回环。数据为联调用 fixture，可直接改这里。
 * 覆盖：列表加载契约（总开关 + 授权 + 规则摘要）与规则模板详情（按 id 查询 / 保存）。
 * 标题标记、变量目录属于公共静态配置，在 mocks/handlers/catalog 里，不在这里。
 */
import type {
  NotificationPermission,
  NotificationRuleSummary,
  NotificationSettings,
  NotificationTemplateDetail,
  NotificationTemplateUpdate,
} from '@/types/domain/notifications';

import { registerMockHandler } from '../registry';

/** 进程内规则：列表摘要字段 + 当前模板 + 该事件默认模板。 */
interface MockRule {
  id: string;
  title: string;
  subTitle: string;
  status: boolean;
  templateType: 'default' | 'custom';
  channel: string;
  markerId: string;
  titleTemplate: string;
  bodyTemplate: string;
  defaultMarkerId: string;
  defaultTitleTemplate: string;
  defaultBodyTemplate: string;
}

let notificationsEnabled = true;
let authorization: NotificationPermission = 'not_requested';

const rules: MockRule[] = [
  {
    id: 'probe.model_failed',
    title: '模型探测故障',
    subTitle: '模型达到稳定故障判定时发送系统通知',
    status: true,
    templateType: 'default',
    channel: '系统通知',
    markerId: 'warning',
    titleTemplate: '{{model}} 探测故障',
    bodyTemplate: '{{model}} 在 {{endpoint}} 连续探测失败，已判定为稳定故障（{{time}}）。',
    defaultMarkerId: 'warning',
    defaultTitleTemplate: '{{model}} 探测故障',
    defaultBodyTemplate: '{{model}} 在 {{endpoint}} 连续探测失败，已判定为稳定故障（{{time}}）。',
  },
  {
    id: 'probe.model_recovered',
    title: '模型探测恢复',
    subTitle: '模型从稳定故障恢复时发送系统通知',
    status: true,
    templateType: 'custom',
    channel: '系统通知',
    markerId: 'success',
    titleTemplate: '{{model}} 恢复啦',
    bodyTemplate: '好消息：{{model}} 已在 {{time}} 恢复正常，端点 {{endpoint}}。',
    defaultMarkerId: 'success',
    defaultTitleTemplate: '{{model}} 已恢复',
    defaultBodyTemplate: '{{model}} 在 {{endpoint}} 恢复正常响应（{{time}}）。',
  },
];

function summaryOf(rule: MockRule): NotificationRuleSummary {
  const { id, title, subTitle, status, templateType, channel } = rule;
  return { id, title, subTitle, status, templateType, channel };
}

function detailOf(rule: MockRule): NotificationTemplateDetail {
  return {
    ...summaryOf(rule),
    markerId: rule.markerId,
    titleTemplate: rule.titleTemplate,
    bodyTemplate: rule.bodyTemplate,
    defaultMarkerId: rule.defaultMarkerId,
    defaultTitleTemplate: rule.defaultTitleTemplate,
    defaultBodyTemplate: rule.defaultBodyTemplate,
  };
}

function currentSettings(): NotificationSettings {
  return structuredClone({ notificationsEnabled, authorization, rules: rules.map(summaryOf) });
}

registerMockHandler('get_notification_settings', () => currentSettings());

registerMockHandler('update_notification_settings', (args) => {
  const { settings } = args as { settings: NotificationSettings };
  notificationsEnabled = settings.notificationsEnabled;
  for (const next of settings.rules) {
    const target = rules.find((rule) => rule.id === next.id);
    if (target) target.status = next.status;
  }
  return currentSettings();
});

registerMockHandler('request_notification_permission', () => {
  authorization = 'granted';
  return authorization;
});

registerMockHandler('send_test_notification', () => undefined);

registerMockHandler('open_notification_settings', () => undefined);

registerMockHandler('get_notification_template', (args) => {
  const { id } = args as { id: string };
  const rule = rules.find((item) => item.id === id);
  if (!rule) throw new Error(`未找到通知模板详情：${id}`);
  return structuredClone(detailOf(rule));
});

registerMockHandler('update_notification_template', (args) => {
  const { id, update } = args as { id: string; update: NotificationTemplateUpdate };
  const rule = rules.find((item) => item.id === id);
  if (!rule) throw new Error(`未找到通知模板详情：${id}`);
  rule.templateType = update.templateType;
  rule.markerId = update.markerId;
  rule.titleTemplate = update.titleTemplate;
  rule.bodyTemplate = update.bodyTemplate;
  return structuredClone(detailOf(rule));
});
