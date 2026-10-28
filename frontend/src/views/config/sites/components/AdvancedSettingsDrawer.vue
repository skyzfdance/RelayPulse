<script setup lang="ts">
import type { Connection } from '@/types/domain/sites';

import { computed, onScopeDispose, ref } from 'vue';

import { saveSiteConnection } from '@/api/config/sites';
import { useSiteDetailsStore, useSiteTasksStore } from '@/stores/site';

import SiteAdvancedForm from './SiteAdvancedForm.vue';

const emit = defineEmits<{ close: [] }>();
const detailsStore = useSiteDetailsStore();
const taskStore = useSiteTasksStore();
const site = detailsStore.selectedSiteDetail!;
// 每次打开都会重新挂载，只从当前站点已保存的配置复制草稿。
const connection = ref<Connection>({
  ...site.connection,
  headers: site.connection.headers.map((header) => ({ ...header })),
});
const advancedForm = ref<InstanceType<typeof SiteAdvancedForm>>();
const saving = ref(false);
const disabled = computed(() => saving.value || Boolean(taskStore.tasks[site.id]));
let disposed = false;
onScopeDispose(() => {
  disposed = true;
});

async function save() {
  if (
    disposed ||
    !advancedForm.value ||
    disabled.value ||
    !taskStore.register(site.id, { type: 'save-connection' })
  )
    return;
  saving.value = true;
  try {
    await advancedForm.value.validate();
    if (disposed) return;
    const input = JSON.parse(JSON.stringify(connection.value));
    const saved = await saveSiteConnection(site.id, input);
    detailsStore.setSiteDetail(saved);
    if (!disposed) emit('close');
  } catch {
    // 表单展示校验错误，API 提示请求错误；失败时保留本次打开的草稿。
  } finally {
    taskStore.remove(site.id);
    saving.value = false;
  }
}
</script>
<template>
  <a-drawer
    open
    size="large"
    :mask-closable="false"
    :closable="{ disabled: saving }"
    :keyboard="!saving"
    @close="!saving && emit('close')"
  >
    <template #title>
      <div>
        <strong>站点高级设置</strong>
        <div class="mt-0.5 text-xs font-normal text-rp-tertiary">{{ site.name }}</div>
      </div>
    </template>
    <template #extra>
      <a-space>
        <a-button :disabled="saving" @click="!saving && emit('close')">取消</a-button>
        <a-button type="primary" :loading="saving" :disabled="disabled" @click="save"
          >保存</a-button
        >
      </a-space>
    </template>
    <SiteAdvancedForm
      ref="advancedForm"
      v-model="connection"
      :base-url="site.baseUrl"
      :disabled="disabled"
    />
  </a-drawer>
</template>
