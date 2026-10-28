<script setup lang="ts">
import { RouterView, useRoute } from 'vue-router';

const route = useRoute();
</script>

<template>
  <a-layout-content class="flex min-h-0 min-w-0 flex-col overflow-hidden bg-rp-surface">
    <!-- 头部：标题与子标题由路由 meta（mainTitle / subTitle）驱动，与侧栏 logo 区同高 -->
    <header
      class="flex min-h-19 flex-none items-center justify-between border-b border-rp-line px-6 py-3"
    >
      <div>
        <!-- antdv reset（无层）会给 h1/p 设默认外边距，压过分层工具类，故需 ! -->
        <h1 class="mb-1! text-xl leading-7 font-semibold">{{ route.meta.mainTitle }}</h1>
        <p v-if="route.meta.subTitle" class="m-0! text-xs leading-5 text-rp-tertiary">
          {{ route.meta.subTitle }}
        </p>
      </div>
    </header>

    <!-- 下方业务内容区：暂留空，由各路由视图填充 -->
    <div class="min-h-0 flex-1 overflow-y-auto">
      <RouterView />
    </div>
  </a-layout-content>
</template>
