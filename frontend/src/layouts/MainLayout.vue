<script setup lang="ts">
import LayoutContent from './components/LayoutContent.vue';
import LayoutSider from './components/LayoutSider.vue';
</script>

<template>
  <a-layout class="h-screen">
    <LayoutSider />
    <LayoutContent />
  </a-layout>
</template>
