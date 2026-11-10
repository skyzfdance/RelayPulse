<script setup lang="ts">
import { computed } from 'vue';

import { outputBlocks } from './iq-output-renderer';

import IqOutputCode from './IqOutputCode.vue';

import 'katex/dist/katex.min.css';

const props = defineProps<{ content: string }>();
const blocks = computed(() => outputBlocks(props.content));
</script>

<template>
  <div class="iq-output-content min-w-0 text-sm leading-7 wrap-anywhere">
    <template v-for="(block, index) in blocks" :key="index">
      <!-- Only sanitized Markdown/KaTeX output enters the host document. -->
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div v-if="block.kind === 'markdown'" v-html="block.html" />
      <IqOutputCode v-else :language="block.language" :content="block.content" />
    </template>
  </div>
</template>

<style scoped>
.iq-output-content :deep(p) {
  margin: 0 0 12px;
}
.iq-output-content :deep(h1),
.iq-output-content :deep(h2),
.iq-output-content :deep(h3) {
  margin: 20px 0 12px;
  font-size: 1.1em;
  font-weight: 600;
}
.iq-output-content :deep(ul),
.iq-output-content :deep(ol) {
  padding-left: 24px;
  margin-block: 12px;
}
.iq-output-content :deep(ul) {
  list-style-type: disc;
}
.iq-output-content :deep(ol) {
  list-style-type: decimal;
}
.iq-output-content :deep(table) {
  display: block;
  max-width: 100%;
  overflow: auto;
  border-collapse: collapse;
  margin-block: 16px;
}
.iq-output-content :deep(td),
.iq-output-content :deep(th) {
  border: 1px solid var(--ant-color-border-secondary);
  padding: 8px 12px;
}
.iq-output-content :deep(th) {
  background: var(--ant-color-fill-quaternary);
}
.iq-output-content :deep(blockquote) {
  margin-inline: 0;
  border-left: 3px solid var(--ant-color-border);
  padding-left: 16px;
  color: var(--ant-color-text-secondary);
}
.iq-output-content :deep(.katex-display) {
  overflow-x: auto;
  overflow-y: hidden;
  padding-block: 4px;
}
.iq-output-content :deep(code) {
  font-family: monospace;
}
</style>
