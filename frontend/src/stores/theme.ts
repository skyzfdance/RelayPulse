import type { ResolvedTheme, ThemeMode } from '@/types/theme';

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

const STORAGE_KEY = 'relaypulse-vue-theme-mode';

function readStoredMode(): ThemeMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === 'system' || stored === 'light' || stored === 'dark' ? stored : 'system';
}

/**
 * 明暗主题状态。mode 是用户选择（system 跟随操作系统），resolvedTheme 是最终生效值，
 * 同步到 <html data-theme="...">；Tailwind 侧通过 @custom-variant dark 消费该属性。
 */
export const useThemeStore = defineStore('vue-theme', () => {
  const mode = ref<ThemeMode>(readStoredMode());
  const systemDark = ref(window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false);

  const resolvedTheme = computed<ResolvedTheme>(() =>
    mode.value === 'dark' || (mode.value === 'system' && systemDark.value) ? 'dark' : 'light',
  );
  const isDark = computed(() => resolvedTheme.value === 'dark');

  function setMode(next: ThemeMode) {
    mode.value = next;
    localStorage.setItem(STORAGE_KEY, next);
  }

  /** system → light → dark 循环切换。 */
  function cycleMode() {
    if (mode.value === 'system') {
      setMode('light');
    } else if (mode.value === 'light') {
      setMode('dark');
    } else {
      setMode('system');
    }
  }

  window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener('change', (event) => {
    systemDark.value = event.matches;
  });

  watch(
    resolvedTheme,
    (next) => {
      document.documentElement.setAttribute('data-theme', next);
    },
    { immediate: true },
  );

  return { mode, systemDark, resolvedTheme, isDark, setMode, cycleMode };
});
