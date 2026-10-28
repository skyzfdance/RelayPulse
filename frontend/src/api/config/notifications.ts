/**
 * 消息通知的具名后端方法（config/notifications）。
 * 统一经 callBackend；浏览器 mock 命中 mocks/handlers/config/notifications。
 * 标题标记、变量目录属于公共静态配置，走 api/catalog，不在这里。
 */
import type {
  NotificationPermission,
  NotificationSettings,
  NotificationTemplateDetail,
  NotificationTemplateUpdate,
  NotificationTestTemplate,
} from '@/types/domain/notifications';

import { callBackend } from '../call-backend';

/** 读取通知设置（总开关 + 是否已授权 + 规则摘要列表）。读操作静默，失败弹提示，loading 交本层统一控制。 */
export function fetchNotificationSettings(): Promise<NotificationSettings> {
  return callBackend<NotificationSettings>('get_notification_settings', undefined, {
    successFeedback: 'none',
    loading: true,
  });
}

/** 保存通知设置（全量声明式：整份 settings 覆盖）。静默成功，失败走 message。 */
export function updateNotificationSettings(
  settings: NotificationSettings,
): Promise<NotificationSettings> {
  return callBackend<NotificationSettings>('update_notification_settings', { settings });
}

/** 按规则 id 查询模板详情（列表项 + 当前模板 + 该事件默认模板）。读操作静默，失败走 message。 */
export function fetchNotificationTemplateDetail(id: string): Promise<NotificationTemplateDetail> {
  return callBackend<NotificationTemplateDetail>(
    'get_notification_template',
    { id },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

/** 保存某规则的模板（模板类型 + 标记/标题/正文），返回最新详情。成功提示，失败走 message。 */
export function updateNotificationTemplate(
  id: string,
  update: NotificationTemplateUpdate,
): Promise<NotificationTemplateDetail> {
  return callBackend<NotificationTemplateDetail>(
    'update_notification_template',
    { id, update },
    { successText: '模板已保存', failFeedback: 'message' },
  );
}

/** 请求系统通知授权，返回最新授权状态；请求完成后弹窗提醒重启软件。 */
export function requestNotificationPermission(): Promise<NotificationPermission> {
  return callBackend<NotificationPermission>('request_notification_permission', undefined, {
    successFeedback: 'modal',
    successText: '完成授权后，请重启软件。',
  });
}

/** 发送测试通知。统一接口：入参即要渲染发送的模板内容（系统通知行传固定样例，模板抽屉传当前预览内容）。 */
export function sendTestNotification(template: NotificationTestTemplate): Promise<void> {
  return callBackend<void>(
    'send_test_notification',
    { template },
    { successText: '测试通知已发送到系统通知中心' },
  );
}

/** 打开系统通知设置（授权被拒时引导用户到系统设置开启）。 */
export function openNotificationSettings(): Promise<void> {
  return callBackend<void>('open_notification_settings', undefined, { successFeedback: 'none' });
}
