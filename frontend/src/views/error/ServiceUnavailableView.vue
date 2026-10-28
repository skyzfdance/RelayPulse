<script setup lang="ts">
/** 重试：去掉 mock 场景参数后整页重载。 */
function reload() {
  const url = new URL(window.location.href);
  url.searchParams.delete('scenario');
  window.location.assign(url);
}
</script>

<template>
  <!-- antdv-next 的 Result 没有 503 预设状态，用 warning 状态组合出 503 页 -->
  <a-result status="warning" title="503" sub-title="服务暂时不可用，请稍后重试。">
    <template #extra>
      <a-button type="primary" @click="reload">刷新重试</a-button>
    </template>
  </a-result>
</template>
