<script setup lang="ts">
/**
 * 设置项卡片：标题 + 描述 + 控件插槽，控件自动吸底对齐。
 * 用于 SettingCardGrid 内部，支持开关、下拉、分段控制器及复杂按钮组。
 */
interface Props {
  /** 设置项标题 */
  title: string;
  /** 设置项描述（可选），也可通过 #description 插槽自定义 */
  description?: string;
  /** 控件对齐方式：start(左对齐) | end(右对齐) | stretch(拉伸填满，默认) */
  controlAlign?: 'start' | 'end' | 'stretch';
}

withDefaults(defineProps<Props>(), {
  controlAlign: 'stretch',
  description: undefined,
});
</script>

<template>
  <div class="flex flex-col gap-3.5 rounded-xl border border-rp-line bg-rp-surface p-4.5">
    <!-- 标题区：支持插槽覆盖 -->
    <div class="flex flex-col gap-1">
      <slot name="title">
        <strong class="text-sm">{{ title }}</strong>
      </slot>
      <slot name="description">
        <span v-if="description" class="text-xs text-rp-tertiary">{{ description }}</span>
      </slot>
    </div>

    <!-- 控件区：吸底对齐，支持任意内容 -->
    <div
      class="mt-auto flex"
      :class="{
        'justify-start': controlAlign === 'start',
        'justify-end': controlAlign === 'end',
        'justify-stretch': controlAlign === 'stretch',
      }"
    >
      <slot />
    </div>
  </div>
</template>
