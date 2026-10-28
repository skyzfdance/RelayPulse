<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { computed } from 'vue';

import { useSiteDetailsStore, useSiteStore } from '@/stores/site';

import { parameterEnabled } from '../request-parameters';
import { healthColors, healthLabels } from '../site-policy';
import { useSiteDetail } from '../use-site-detail';

import AddModelModal from './AddModelModal.vue';
import AdvancedSettingsDrawer from './AdvancedSettingsDrawer.vue';
import ModelList from './ModelList.vue';
import ModelProbeSettingsDrawer from './ModelProbeSettingsDrawer.vue';
import ModelSyncModal from './ModelSyncModal.vue';
import SiteBasicForm from './SiteBasicForm.vue';
import SiteProbePolicyDrawer from './SiteProbePolicyDrawer.vue';
import AppIcon from '@/components/AppIcon.vue';

const store = useSiteStore();
const { sites } = storeToRefs(store);
const detailsStore = useSiteDetailsStore();
const {
  selectedSiteDetail: site,
  siteDetailsViewState,
  siteDetailsLoadError,
} = storeToRefs(detailsStore);
const {
  draft,
  dirty,
  formError,
  siteBusy,
  savingBasic,
  openAdvancedSettings,
  openSitePolicy,
  openAddModel,
  openModelSettings,
  saveSelectedSite,
  advancedOpen,
  sitePolicyOpen,
  addModelOpen,
  editingModel,
  closeModelSettings,
  createModel,
  synchronizeModels,
  confirmModelSync,
  probeModels,
  setModelEnabled,
  removeModel,
  syncConfirmation,
} = useSiteDetail();
const status = computed(() => sites.value.find((item) => item.id === site.value?.id));
const enabledParameterCount = computed(() => {
  const parameters = site.value?.policy.parameters ?? [];
  return parameters.filter((parameter) => parameterEnabled(parameters, parameter)).length;
});
</script>
<template>
  <div
    v-if="siteDetailsViewState === 'loading'"
    class="min-w-0 flex-1 bg-rp-surface px-6 py-4 max-[1180px]:px-4.5"
    role="status"
    aria-label="正在加载站点详情"
    aria-busy="true"
  >
    <a-skeleton active loading round :paragraph="{ rows: 10 }" />
  </div>
  <div
    v-else-if="siteDetailsViewState === 'error'"
    class="grid min-w-0 flex-1 place-items-center bg-rp-surface"
    role="alert"
  >
    <a-result status="error" title="站点详情加载失败" :sub-title="siteDetailsLoadError">
      <template #extra>
        <a-button type="primary" @click="detailsStore.loadSiteDetails()">重新加载</a-button>
      </template>
    </a-result>
  </div>
  <div
    v-else-if="siteDetailsViewState === 'ready' && site && draft"
    class="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-rp-surface"
  >
    <div class="min-h-0 flex-1 overflow-y-auto px-6 py-4 max-[1180px]:px-4.5">
      <div class="mb-4 flex min-h-8 items-center justify-between gap-3">
        <div class="flex min-w-0 items-center gap-2">
          <span
            v-if="status"
            class="size-1.75 flex-none rounded-full"
            :style="{ background: healthColors[status.health] }"
            role="img"
            :aria-label="healthLabels[status.health]"
          />
          <h2 class="m-0! truncate text-lg! leading-6.5! font-normal!">{{ site.name }}</h2>
          <a-tag color="blue" class="m-0!">优先级 {{ status?.priority }}</a-tag>
          <a-tag :color="site.enabled ? 'green' : undefined" class="m-0!">{{
            site.enabled ? '小面板显示' : '小面板隐藏'
          }}</a-tag>
        </div>
        <a-popconfirm
          title="删除站点？"
          description="站点下的模型和独立设置也会一并删除。"
          ok-text="删除"
          cancel-text="取消"
          :disabled="siteBusy"
          :ok-button-props="{ danger: true }"
          @confirm="store.removeSite()"
        >
          <a-button type="link" danger :disabled="siteBusy">删除站点</a-button>
        </a-popconfirm>
      </div>
      <a-alert v-if="formError" class="mb-4" type="error" :title="formError" show-icon />
      <section class="site-section">
        <div class="section-header">
          <div>
            <h3>基本信息</h3>
            <p>站点名称、协议和访问凭据</p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <a-button :disabled="siteBusy" @click="openAdvancedSettings">高级设置</a-button>
            <a-button
              type="primary"
              :disabled="!dirty || siteBusy"
              :loading="savingBasic"
              @click="saveSelectedSite"
              >保存变更</a-button
            >
          </div>
        </div>
        <SiteBasicForm
          v-model="draft"
          :credential-configured="site.credentialConfigured"
          :disabled="siteBusy"
        />
      </section>
      <section class="site-section">
        <div class="section-header">
          <div>
            <h3>默认探针策略</h3>
            <p>新启用模型默认继承；单个模型可覆盖</p>
          </div>
          <a-button :disabled="siteBusy" @click="openSitePolicy">编辑默认值</a-button>
        </div>
        <div
          class="flex min-h-16.5 items-center gap-3 rounded-md border border-rp-line bg-rp-subtle px-3.5 py-3"
        >
          <span
            class="grid size-8.5 flex-none place-items-center rounded-md bg-rp-active text-rp-primary"
            ><AppIcon name="ApiOutlined"
          /></span>
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <strong>站点默认策略</strong>
            <span class="truncate text-xs text-rp-tertiary"
              >已启用 {{ enabledParameterCount }} /
              {{ site.policy.parameters.length }} 个请求参数</span
            >
          </div>
        </div>
      </section>
      <section class="site-section">
        <ModelList
          :dirty="dirty"
          @add="openAddModel"
          @settings="openModelSettings"
          @sync="synchronizeModels"
          @probe="probeModels"
          @toggle="setModelEnabled"
          @remove="removeModel"
        />
        <p v-if="dirty" class="mt-3 text-xs text-rp-tertiary">先保存站点更改，再操作模型与探针。</p>
      </section>
    </div>
    <AdvancedSettingsDrawer v-if="advancedOpen" @close="advancedOpen = false" />
    <SiteProbePolicyDrawer v-if="sitePolicyOpen" @close="sitePolicyOpen = false" />
    <AddModelModal v-if="addModelOpen" :create-model="createModel" @close="addModelOpen = false" />
    <ModelProbeSettingsDrawer
      v-if="editingModel"
      :key="editingModel.id"
      :model="editingModel"
      @close="closeModelSettings"
    />
    <ModelSyncModal
      :confirmation="syncConfirmation"
      @confirm="confirmModelSync"
      @close="syncConfirmation = undefined"
    />
  </div>
  <div v-else-if="siteDetailsViewState === 'empty'" class="grid flex-1 place-items-center">
    <a-empty description="请从左侧选择一个站点" />
  </div>
</template>
<style scoped>
.site-section {
  border-bottom: 1px solid var(--ant-color-border-secondary);
  padding-bottom: 12px;
}
.site-section + .site-section {
  padding-top: 12px;
}
.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 10px;
}
.section-header h3 {
  margin: 0;
  font-size: 14px;
  line-height: 22px;
  font-weight: 600;
}
.section-header p {
  margin: 0;
  font-size: 12px;
  line-height: 20px;
  color: var(--ant-color-text-tertiary);
}
</style>
