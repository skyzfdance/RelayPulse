/**
 * 通用设置页编排 hook：读设置、改设置（保存成功后才落到 UI），以及诊断日志的打开/清理/导出。
 * loading 一律交 callBackend / api 层统一控制，本层不另设页面级 loading；失败由 api 层弹提示。
 */
import type { GeneralSettings, LogStatistics } from '@/types/domain/general';
import type { ThemeMode } from '@/types/theme';

import { onMounted, ref } from 'vue';

import {
  clearDiagnosticLogs,
  exportDiagnostics,
  fetchGeneralSettings,
  fetchLogStatistics,
  openLogDirectory,
  updateGeneralSettings,
} from '@/api/config/general';
import { useThemeStore } from '@/stores/theme';

export function useGeneral() {
  const themeStore = useThemeStore();

  const settings = ref<GeneralSettings>();
  const logStats = ref<LogStatistics>();

  async function load(): Promise<void> {
    const [next, stats] = await Promise.all([fetchGeneralSettings(), fetchLogStatistics()]);
    settings.value = next;
    logStats.value = stats;
  }

  /**
   * 改一个设置项：整份提交给后端，保存成功后才用返回值覆盖本地值（失败时值没动过，提示即可）。
   * theme 额外驱动 theme store 立即换肤；保存失败则把主题改回去。
   */
  async function setSetting<K extends keyof GeneralSettings>(
    key: K,
    value: GeneralSettings[K],
  ): Promise<void> {
    const previous = themeStore.mode;
    try {
      settings.value = await updateGeneralSettings({ ...settings.value!, [key]: value });
      if (key === 'theme') themeStore.setMode(value as ThemeMode);
    } catch {
      if (key === 'theme') themeStore.setMode(previous);
    }
  }

  async function clearLogs(): Promise<void> {
    const res = await clearDiagnosticLogs();
    logStats.value = res;
  }

  async function exportPackage(): Promise<void> {
    await exportDiagnostics();
  }

  async function openLogs(): Promise<void> {
    await openLogDirectory();
  }

  onMounted(load);

  return {
    settings,
    logStats,
    load,
    setSetting,
    clearLogs,
    exportPackage,
    openLogs,
  };
}
