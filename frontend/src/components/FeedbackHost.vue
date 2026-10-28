<!--
  提示单例宿主：把 <a-app> 的 App.useApp() 实例（message / notification /
  modal，带主题上下文）注入 useFeedback 单例桥，使 store 与 api 层可以在
  组件外触发提示。本组件自身不渲染任何可见内容。
-->
<script setup lang="ts">
import { App } from 'antdv-next';

import { bindFeedbackInstances } from '@/composables/useFeedback';

const { message, modal, notification } = App.useApp();
bindFeedbackInstances({ message, notification, modal });
</script>

<template>
  <span hidden />
</template>
