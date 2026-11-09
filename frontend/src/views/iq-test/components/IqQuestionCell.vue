<script setup lang="ts">
import type { IqQuestion } from '@/types/domain/iq-test';

import dayjs from 'dayjs';

const props = defineProps<{ question: IqQuestion }>();
const emit = defineEmits<{ view: [] }>();
</script>

<template>
  <div class="min-w-0 py-1">
    <button
      type="button"
      class="question-preview block w-full cursor-pointer border-0 bg-transparent p-0 text-left text-sm leading-5.5 whitespace-pre-wrap wrap-anywhere hover:text-rp-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-rp-primary"
      :class="{ 'text-rp-tertiary': !props.question.enabled }"
      :aria-label="`查看题目 ${question.content}`"
      @click.stop="emit('view')"
    >
      {{ question.content }}
    </button>
    <div class="mt-1.5 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs leading-5">
      <button
        v-if="question.referenceAnswer"
        type="button"
        class="cursor-pointer border-0 bg-transparent p-0 text-rp-primary hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-rp-primary"
        :aria-label="`查看参考答案 ${question.content}`"
        @click.stop="emit('view')"
      >
        有参考答案
      </button>
      <span v-else class="text-rp-tertiary">无参考答案</span>
      <span class="text-rp-tertiary tabular-nums">
        更新于 {{ dayjs(question.updatedAtMs).format('YYYY-MM-DD HH:mm') }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.question-preview {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}
</style>
