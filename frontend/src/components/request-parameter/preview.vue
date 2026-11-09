<script setup lang="ts">
defineOptions({ name: 'RequestParameterPreview' });

withDefaults(defineProps<{ value: string; title?: string; description?: string }>(), {
  title: '有效请求预览',
  description: '按当前启用的参数生成请求',
});
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
    <pre
      :aria-label="title"
      class="mt-3! mb-0! overflow-x-auto rounded-md bg-rp-surface p-3 font-mono text-[11px]! leading-4! whitespace-pre-wrap wrap-anywhere"
      >{{ value }}</pre>
    <slot />
  </section>
</template>
