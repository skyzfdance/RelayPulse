<!-- 全局页面级 loading 遮罩宿主：挂载于应用顶层，渲染 useGlobalLoading 单例状态 -->
<script setup lang="ts">
import { useGlobalLoading } from '@/composables/useGlobalLoading';

const { visible, tip } = useGlobalLoading();
</script>

<template>
  <a-spin :spinning="visible" :description="tip" size="large" fullscreen />
</template>
