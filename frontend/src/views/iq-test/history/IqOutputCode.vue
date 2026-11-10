<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import IqOutputPreview from './IqOutputPreview.vue';

const props = defineProps<{ language: string; content: string }>();
const copied = ref(false);
const copyError = ref(false);
const canPreview = computed(
  () =>
    ['html', 'svg', 'xml'].includes(props.language.toLowerCase()) &&
    /<(?:html|svg|!doctype)/i.test(props.content),
);
const previewing = ref(canPreview.value);
const expanded = ref(false);
watch(
  [() => props.content, () => props.language],
  () => {
    previewing.value = canPreview.value;
    expanded.value = false;
    copied.value = false;
  },
  { flush: 'sync' },
);
async function copy() {
  copyError.value = false;
  try {
    await navigator.clipboard.writeText(props.content);
    copied.value = true;
  } catch {
    copyError.value = true;
  }
}
</script>

<template>
  <section class="my-3 overflow-hidden rounded-md border border-rp-line">
    <div class="flex items-center gap-2 border-b border-rp-line bg-rp-subtle px-3 py-2">
      <span class="flex-1 text-xs text-rp-secondary">{{ language || '代码' }}</span>
      <a-button v-if="previewing" size="small" @click="expanded = true">放大查看</a-button>
      <a-button
        v-if="canPreview"
        size="small"
        :type="previewing ? 'primary' : 'default'"
        @click="previewing = !previewing"
        >{{ previewing ? '查看代码' : '预览' }}</a-button
      >
      <a-button size="small" @click="copy">{{ copied ? '已复制' : '复制代码' }}</a-button>
      <span v-if="copyError" role="status" class="text-xs text-(--ant-color-error)"
        >复制失败，请手动复制</span
      >
    </div>
    <IqOutputPreview v-if="previewing" :content="content" class="h-64 w-full">
      <button
        type="button"
        aria-label="放大作品预览"
        title="放大查看"
        class="absolute inset-0 cursor-zoom-in border-0 bg-transparent focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rp-primary"
        @click="expanded = true"
      />
    </IqOutputPreview>
    <pre
      v-else
      class="m-0! max-h-[560px] overflow-auto bg-rp-subtle p-4 font-mono text-xs! leading-6!"
    ><code>{{ content }}</code></pre>
  </section>
  <a-modal
    v-model:open="expanded"
    title="作品预览"
    width="calc(100vw - 32px)"
    :footer="null"
    :closable="{ 'aria-label': '关闭作品预览' }"
    destroy-on-hidden
    :style="{ top: '16px', paddingBottom: 0, margin: '0 auto' }"
    :styles="{
      wrapper: { overflow: 'hidden' },
      container: { height: 'calc(100dvh - 32px)', display: 'flex', flexDirection: 'column' },
      header: { flexShrink: 0 },
      body: { flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden' },
    }"
  >
    <IqOutputPreview v-if="expanded" :content="content" interactive class="min-h-0 flex-1" />
  </a-modal>
</template>
