import type { ThemeMode } from '../theme';

export interface GeneralSettings {
  launchAtStartup: boolean;
  continueInBackground: boolean;
  showTrayIcon: boolean;
  theme: ThemeMode;
  language: 'zh-CN';
  historyRetentionDays: number;
}

/** 诊断日志占用统计（字节）。 */
export interface LogStatistics {
  totalBytes: number;
  fileCount: number;
}
