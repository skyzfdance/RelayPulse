/**
 * 通用设置与诊断日志命令的浏览器 mock（config/general）。
 *
 * 通用设置的初始样例、日志统计与命令处理均由本模块维护，修改仅保留在当前会话。
 */
import type { GeneralSettings, LogStatistics } from '@/types/domain/general';

import { registerMockHandler } from '../registry';

let settings: GeneralSettings = {
  launchAtStartup: true,
  continueInBackground: true,
  showTrayIcon: true,
  theme: 'system',
  language: 'zh-CN',
  historyRetentionDays: 30,
};

let logStatistics: LogStatistics = {
  totalBytes: 1_835_008,
  fileCount: 2,
};

export function getMockGeneralSettings(): GeneralSettings {
  return structuredClone(settings);
}

registerMockHandler('get_general_settings', getMockGeneralSettings);

registerMockHandler('update_general_settings', (args) => {
  const { settings: next } = args as { settings: GeneralSettings };
  settings = structuredClone(next);
  return structuredClone(settings);
});

registerMockHandler('get_log_statistics', () => structuredClone(logStatistics));

registerMockHandler('clear_diagnostic_logs', () => {
  logStatistics = { totalBytes: 192, fileCount: 1 };
  return structuredClone(logStatistics);
});

registerMockHandler('export_diagnostics', () => true);

registerMockHandler('open_log_directory', () => undefined);
