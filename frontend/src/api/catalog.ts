/**
 * 后端静态公共配置的统一出口（catalog）。
 *
 * 这里放「不入库、不常变、跨特性复用」的配置类数据：通知标题标记、模板变量、
 * 站点协议目录；以后同类公共配置都加到这里，不要散落到各 feature 的 api 目录下。
 * 本层保持无状态（只发请求）；若将来确需应用级缓存，用 Pinia store 承载，别在这里存。
 * 统一经 callBackend；浏览器 mock 命中 mocks/handlers/catalog。
 */
import type { NotificationMarker, NotificationVariable } from '@/types/domain/notifications';
import type { SiteProtocolOption } from '@/types/domain/sites';

import { callBackend } from './call-backend';

/** 站点支持的接口协议，按接口返回顺序展示。 */
export function fetchSiteProtocols(): Promise<SiteProtocolOption[]> {
  return callBackend<SiteProtocolOption[]>('get_site_protocols', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}

/** 通知「标题标记」目录（符号/名称/色调）。读操作静默，失败走 message。 */
export function fetchNotificationMarkers(): Promise<NotificationMarker[]> {
  return callBackend<NotificationMarker[]>('get_notification_markers', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}

/** 通知模板「变量」目录（{{key}} 的含义/示例）。读操作静默，失败走 message。 */
export function fetchNotificationVariables(): Promise<NotificationVariable[]> {
  return callBackend<NotificationVariable[]>('get_notification_variables', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}
