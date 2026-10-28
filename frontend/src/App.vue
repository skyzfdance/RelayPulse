<script setup lang="ts">
import { theme as antdTheme } from 'antdv-next';
import zhCN from 'antdv-next/locale/zh_CN';
import { computed } from 'vue';
import { RouterView } from 'vue-router';

import { provideMainWindowNavigation } from '@/composables/useMainWindowNavigation';
import { useThemeStore } from '@/stores/theme';

import FeedbackHost from '@/components/FeedbackHost.vue';
import GlobalLoadingHost from '@/components/GlobalLoadingHost.vue';

const themeStore = useThemeStore();
provideMainWindowNavigation();

const themeConfig = computed(() => ({
  cssVar: true,
  algorithm: themeStore.isDark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
}));
</script>

<template>
  <a-config-provider :locale="zhCN" :theme="themeConfig">
    <a-app>
      <RouterView />
      <!-- 全局提示与页面级 loading 的渲染宿主（单例基础设施，无业务逻辑） -->
      <FeedbackHost />
      <GlobalLoadingHost />
    </a-app>
  </a-config-provider>
</template>
