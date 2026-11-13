<script setup lang="ts">
import type { RequestPreview } from '@/types/request-parameters';

defineOptions({ name: 'RequestParameterPreview' });

withDefaults(
  defineProps<{ value: RequestPreview | string; title?: string; description?: string }>(),
  {
    title: '有效请求预览',
    description: '按当前启用的参数生成请求',
  },
);
</script>

<template>
  <section>
    <div class="flex items-center justify-between gap-4">
      <div>
        <h3 class="m-0! text-sm font-semibold leading-5.5">{{ title }}</h3>
        <p class="m-0! text-xs leading-5 text-rp-tertiary">{{ description }}</p>
      </div>
      <slot name="extra" />
    </div>
    <div :aria-label="title" class="mt-3">
      <p v-if="typeof value === 'string'" role="status" class="m-0! text-xs text-rp-secondary">
        {{ value }}
      </p>
      <div v-else class="flex flex-col gap-4">
        <section aria-label="请求地址">
          <h4 class="m-0! mb-2! text-xs font-semibold">请求地址</h4>
          <pre class="request-preview-code font-mono">{{ value.method }} {{ value.address }}</pre>
        </section>
        <section aria-label="请求头 Headers">
          <h4 class="m-0! mb-2! text-xs font-semibold">请求头 Headers</h4>
          <pre aria-label="请求头 JSON" class="request-preview-code font-mono">{{
            JSON.stringify({ ...value.headers.defaults, ...value.headers.configured }, null, 2)
          }}</pre>
        </section>
        <section aria-label="请求体 Body">
          <h4 class="m-0! mb-2! text-xs font-semibold">请求体 Body</h4>
          <pre aria-label="请求体 JSON" class="request-preview-code font-mono">{{
            JSON.stringify(value.body, null, 2)
          }}</pre>
        </section>
      </div>
    </div>
    <slot />
  </section>
</template>

<style scoped>
.request-preview-code {
  margin: 0;
  overflow-x: auto;
  border-radius: var(--ant-border-radius);
  border: 1px solid var(--ant-color-border-secondary);
  background: var(--ant-color-fill-quaternary);
  color: var(--ant-color-text);
  padding: 12px;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
