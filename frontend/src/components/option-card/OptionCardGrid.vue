<script setup lang="ts">
import { computed } from 'vue';

/**
 * 设置项卡片栅格容器：自适应列数或固定列数，用于承载多个 SettingCard。
 * 通过 CSS Grid 实现响应式，根据 minWidth 自动计算列数，避免大屏空隙。
 */
interface Props {
  /** 列数：'auto' 为自适应（默认），数字为固定列数 */
  columns?: number | 'auto';
  /** 自适应模式下，每张卡片的最小宽度（px），默认 300 */
  minWidth?: number;
  /** 卡片间距（px），默认 16 */
  gutter?: number;
}

const props = withDefaults(defineProps<Props>(), {
  columns: 'auto',
  minWidth: 300,
  gutter: 16,
});

const gridStyle = computed(() => {
  const gap = `${props.gutter}px`;
  const columns =
    props.columns === 'auto'
      ? `repeat(auto-fill, minmax(${props.minWidth}px, 1fr))`
      : `repeat(${props.columns}, 1fr)`;
  return { display: 'grid', gap, gridTemplateColumns: columns };
});
</script>

<template>
  <div class="setting-card-grid" :style="gridStyle">
    <slot />
  </div>
</template>
