<script setup lang="ts">
import type { TooltipRef } from 'antdv-next';

import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';

import { useOverflowTooltip } from '@/composables/useOverflowTooltip';

const { active, hide } = useOverflowTooltip();
const tooltip = ref<TooltipRef>();
const anchorStyle = computed(() => {
  const rect = active.value?.target.getBoundingClientRect();
  if (!rect) return {};
  return {
    position: 'fixed' as const,
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
    pointerEvents: 'none' as const,
    opacity: 0,
  };
});
const popupStyles = {
  root: { pointerEvents: 'none' as const, maxWidth: 'min(480px, calc(100vw - 24px))' },
  container: { whiteSpace: 'pre-wrap' as const, overflowWrap: 'anywhere' as const },
};

watch(
  active,
  async (entry, _previous, onCleanup) => {
    if (!entry) return;
    const dismiss = () => hide(entry.target);
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') dismiss();
    };
    // 表格内部滚动也会改变代理锚点的位置，直接关闭，避免提示停留在旧位置。
    window.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('blur', dismiss);
    window.addEventListener('keydown', keydown);
    onCleanup(() => {
      window.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('blur', dismiss);
      window.removeEventListener('keydown', keydown);
    });
    await nextTick();
    tooltip.value?.forceAlign();
  },
  { flush: 'post' },
);
onBeforeUnmount(() => hide());
</script>

<template>
  <Teleport v-if="active" to="body">
    <a-tooltip
      ref="tooltip"
      :open="true"
      :title="active.text"
      :trigger="[]"
      :unique="false"
      :destroy-on-hidden="true"
      placement="topLeft"
      :styles="popupStyles"
    >
      <!-- 代理锚点仅用于定位，不移动或重新托管原来的 Vue DOM。 -->
      <span class="rp-overflow-tooltip-anchor" :style="anchorStyle" aria-hidden="true" />
    </a-tooltip>
  </Teleport>
</template>
