<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { onMounted, ref } from 'vue';

import { onMainWindowRefresh } from '@/composables/useMainWindowNavigation';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import AddSiteModal from './components/AddSiteModal.vue';
import SiteDetail from './components/SiteDetail.vue';
import SiteList from './components/SiteList.vue';
import AppIcon from '@/components/AppIcon.vue';

const addSiteOpen = ref(false);
const store = useSiteStore();
const { sitesViewState, sitesLoadError } = storeToRefs(store);

onMounted(() => store.loadSite());
onMainWindowRefresh('/config/sites', async () => {
  await store.loadSite({ autoLoadDetails: false });
  const id = store.selectedSiteId;
  if (id && !useSiteTasksStore().tasks[id])
    await useSiteDetailsStore().loadSiteDetails({ silent: true });
});
</script>
<template>
  <div class="flex h-full min-h-0">
    <a-skeleton
      v-if="sitesViewState === 'loading'"
      active
      loading
      round
      :paragraph="{ rows: 8 }"
      class="px-6 py-4"
      role="status"
      aria-label="正在加载站点"
      aria-busy="true"
    />
    <div v-else-if="sitesViewState === 'error'" class="grid flex-1 place-items-center px-6">
      <div class="text-center" role="alert">
        <h2 class="text-lg">站点列表加载失败</h2>
        <p class="text-rp-secondary">{{ sitesLoadError }}</p>
        <a-button type="primary" @click="store.loadSite()">重新加载</a-button>
      </div>
    </div>
    <div v-else-if="sitesViewState === 'empty'" class="grid flex-1 place-items-center px-6 pb-10">
      <a-empty>
        <template #image>
          <AppIcon name="FileAddOutlined" class="text-7xl text-rp-quaternary" />
        </template>
        <template #description>
          <div class="max-w-130">
            <h2 class="mt-2 mb-1 text-lg">还没有站点</h2>
            <p class="m-0 leading-5.5 text-rp-secondary">
              先添加一个 OpenAI 兼容、Anthropic、OpenAI Responses 或自定义 HTTP
              站点，连接成功后即可选择需要探测的模型。
            </p>
          </div>
        </template>
        <a-button type="primary" @click="addSiteOpen = true">
          <template #icon><AppIcon name="PlusOutlined" /></template>
          添加第一个站点
        </a-button>
      </a-empty>
    </div>
    <template v-else>
      <SiteList @add="addSiteOpen = true" />
      <SiteDetail />
    </template>
    <AddSiteModal v-if="addSiteOpen" @close="addSiteOpen = false" />
  </div>
</template>
