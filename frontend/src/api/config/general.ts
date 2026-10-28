/**
 * 通用设置与诊断日志的具名后端方法（config/general）。
 * 统一经 callBackend；浏览器 mock 命中 mocks/handlers/config/general。
 * 设置为自包含契约（get/update 直接收发 GeneralSettings，不读全局快照），与 config/notifications 一致。
 */
import type { GeneralSettings, LogStatistics } from '@/types/domain/general';

import { callBackend } from '../call-backend';

/** 读取通用设置。读操作静默，失败弹提示，loading 交本层统一控制。 */
export function fetchGeneralSettings(): Promise<GeneralSettings> {
  return callBackend<GeneralSettings>('get_general_settings', undefined, {
    successFeedback: 'none',
    loading: true,
  });
}

/** 保存通用设置（全量声明式：整份 settings 覆盖）。静默成功，失败走 message。 */
export function updateGeneralSettings(settings: GeneralSettings): Promise<GeneralSettings> {
  return callBackend<GeneralSettings>(
    'update_general_settings',
    { settings },
    { failFeedback: 'message' },
  );
}

/** 读取诊断日志占用统计。读操作静默，失败弹提示，loading 交本层统一控制。 */
export function fetchLogStatistics(): Promise<LogStatistics> {
  return callBackend<LogStatistics>('get_log_statistics', undefined, {
    successFeedback: 'none',
    loading: true,
  });
}

/** 清理诊断日志，返回清理后的最新统计。静默成功（提示由调用方给），失败走 modal。 */
export function clearDiagnosticLogs(): Promise<LogStatistics> {
  return callBackend<LogStatistics>('clear_diagnostic_logs', undefined, {
    successFeedback: 'message',
    successText: '日志清理成功！',
  });
}

/** 导出诊断包：成功返回 true 并提示；取消或失败由后端返回错误，统一走 modal。 */
export function exportDiagnostics(): Promise<boolean> {
  return callBackend<boolean>('export_diagnostics', undefined, {
    successFeedback: 'message',
    successText: '诊断包已导出！',
  });
}

/** 在系统文件管理器中打开本地日志目录。成功提示，失败走 message。 */
export function openLogDirectory(): Promise<void> {
  return callBackend<void>('open_log_directory', undefined, {
    successFeedback: 'none',
  });
}
