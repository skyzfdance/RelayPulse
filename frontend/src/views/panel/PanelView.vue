<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { openPanelSettings, openPanelSites } from '@/api/panel';
import { useThemeStore } from '@/stores/theme';

import { usePanelSnapshot } from './usePanelSnapshot';
import { usePanelWindow } from './usePanelWindow';

import PanelSurface from './components/PanelSurface.vue';

const { snapshot, refreshing, loadError, loadSnapshot, refresh } = usePanelSnapshot();
const themeStore = useThemeStore();
// 只应用已通过 revision 校验的快照，晚到的旧响应不能把主题切回去。
watch(
  () => snapshot.value?.theme,
  (mode) => {
    if (mode) themeStore.setMode(mode);
  },
  { flush: 'sync' },
);
const emit = defineEmits<{ openSettings: []; viewAll: [] }>();
const content = ref<HTMLElement | null>(null);
const { nativeBackdrop } = usePanelWindow(
  content,
  computed(() => themeStore.mode),
);

function focusPanel() {
  // 原生窗口打开时不自动选中第一个按钮；Tab 仍从刷新按钮开始。
  content.value?.focus({ preventScroll: true });
}

onMounted(() => {
  focusPanel();
  window.addEventListener('focus', focusPanel);
});
onBeforeUnmount(() => window.removeEventListener('focus', focusPanel));

async function openSettings() {
  emit('openSettings');
  try {
    await openPanelSettings();
  } catch {
    // 原生动作失败由统一 API 提示，保留面板以便重试。
  }
}

async function viewAll() {
  emit('viewAll');
  try {
    await openPanelSites();
  } catch {
    // 原生动作失败由统一 API 提示，保留面板以便重试。
  }
}
</script>

<template>
  <main ref="content" class="panel-window" tabindex="-1">
    <PanelSurface
      v-if="snapshot"
      class="panel-window-content"
      :snapshot="snapshot"
      :refreshing="refreshing"
      :frosted="nativeBackdrop"
      @refresh="refresh"
      @open-settings="openSettings"
      @view-all="viewAll"
    />
    <section v-else-if="loadError" class="panel-window-content panel-load-state">
      <p role="alert">{{ loadError }}</p>
      <a-button size="small" @click="loadSnapshot">重试</a-button>
    </section>
  </main>
</template>

<style scoped>
.panel-window {
  width: min(100%, 360px);
  margin-inline: auto;
  outline: none;
}

.panel-window-content {
  width: min(100%, 360px);
  margin-inline: auto;
}

.panel-load-state {
  display: grid;
  justify-items: center;
  gap: 12px;
  padding: 24px 16px;
  border: 1px solid var(--ant-color-border-secondary);
  border-radius: 12px;
  background: var(--ant-color-bg-container);
  color: var(--ant-color-text-secondary);
  font-size: 12px;
  text-align: center;
}

.panel-load-state p {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
