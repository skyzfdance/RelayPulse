<script setup lang="ts">
import { computed, onMounted, onScopeDispose, ref, watch } from 'vue';

import { previewDocument } from './iq-output-renderer';

const props = defineProps<{ content: string; interactive?: boolean }>();
// A consistent document viewport keeps thumbnail and enlarged layouts identical.
const documentWidth = 1024;
const documentHeight = ref(768);
const frame = ref<HTMLIFrameElement>();
const viewport = ref<HTMLElement>();
const available = ref({ width: 0, height: 0 });
const channel = ref(crypto.randomUUID());
const document = computed(() => previewDocument(props.content, channel.value));
const scale = computed(() =>
  Math.min(1, available.value.width / documentWidth, available.value.height / documentHeight.value),
);
watch(
  () => props.content,
  () => {
    documentHeight.value = 768;
    channel.value = crypto.randomUUID();
  },
  { flush: 'sync' },
);
function resizePreview(event: MessageEvent) {
  const data = event.data;
  if (
    event.source !== frame.value?.contentWindow ||
    !data ||
    data.type !== 'iq-preview-size' ||
    data.channel !== channel.value ||
    typeof data.height !== 'number' ||
    !Number.isFinite(data.height) ||
    data.height <= 0
  )
    return;
  documentHeight.value = Math.ceil(data.height);
}
let observer: ResizeObserver | undefined;
onMounted(() => {
  const measure = () => {
    const element = viewport.value;
    if (element) available.value = { width: element.clientWidth, height: element.clientHeight };
  };
  observer = new ResizeObserver(measure);
  if (viewport.value) observer.observe(viewport.value);
  measure();
  window.addEventListener('message', resizePreview);
});
onScopeDispose(() => {
  observer?.disconnect();
  window.removeEventListener('message', resizePreview);
});
</script>

<template>
  <div ref="viewport" class="relative grid min-w-0 place-items-center overflow-hidden bg-rp-subtle">
    <div
      class="relative shrink-0"
      :style="{ width: `${documentWidth * scale}px`, height: `${documentHeight * scale}px` }"
    >
      <iframe
        ref="frame"
        :srcdoc="document"
        :style="{
          width: `${documentWidth}px`,
          height: `${documentHeight}px`,
          transform: `scale(${scale})`,
        }"
        :tabindex="interactive ? 0 : -1"
        sandbox="allow-scripts"
        referrerpolicy="no-referrer"
        title="模型生成内容预览"
        class="absolute top-0 left-0 block origin-top-left border-0 bg-white"
        :class="{ 'pointer-events-none': !interactive }"
      />
    </div>
    <slot />
  </div>
</template>
