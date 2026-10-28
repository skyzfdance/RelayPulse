<script setup lang="ts">
import { Empty } from 'antdv-next';
import { computed } from 'vue';

const props = defineProps<{
  kind: 'records' | 'tokens';
  loading: boolean;
  retainedCount: number;
}>();
const emit = defineEmits<{ reset: [] }>();
const description = computed(() => {
  if (props.retainedCount > 0)
    return props.kind === 'records' ? '没有符合条件的探测记录' : '没有符合条件的统计数据';
  return props.kind === 'records' ? '暂无探测记录' : '暂无 Token 统计';
});
</script>
<template>
  <div v-if="loading" class="py-12 text-rp-tertiary" role="status">数据加载中…</div>
  <a-empty v-else class="py-8" :image="Empty.PRESENTED_IMAGE_SIMPLE">
    <template #description>
      <p class="mb-1! text-rp-secondary">{{ description }}</p>
      <p v-if="retainedCount === 0" class="m-0! text-xs text-rp-tertiary">
        正式探测完成后，记录会显示在这里。
      </p>
    </template>
    <a-button v-if="retainedCount > 0" type="link" @click="emit('reset')">重置筛选</a-button>
  </a-empty>
</template>
