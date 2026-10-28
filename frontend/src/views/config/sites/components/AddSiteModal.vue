<script setup lang="ts">
import type { CreateSiteInput, SiteProtocol, SiteProtocolOption } from '@/types/domain/sites';
import type { FormInstance, InputRef } from 'antdv-next';

import { onMounted, onScopeDispose, ref } from 'vue';

import { fetchSiteProtocols } from '@/api/catalog';
import { createSite } from '@/api/config/sites';
import { useSiteDetailsStore, useSiteStore } from '@/stores/site';

const emit = defineEmits<{ close: [] }>();
const store = useSiteStore();
const detailsStore = useSiteDetailsStore();
const protocolOptions = ref<SiteProtocolOption[]>([]);
const loading = ref(false);
const form = ref<FormInstance>();
const nameInput = ref<InputRef>();
let disposed = false;
onScopeDispose(() => {
  disposed = true;
});
const draft = ref<Omit<CreateSiteInput, 'protocol'> & { protocol?: SiteProtocol }>({
  name: '',
  protocol: undefined,
  baseUrl: '',
  secret: '',
  enabled: true,
});
onMounted(async () => {
  try {
    const options = await fetchSiteProtocols();
    if (disposed) return;
    protocolOptions.value = options;
    draft.value.protocol = options[0]?.value;
  } catch {
    // 接口已提示；不回退到前端写死的协议列表。
  }
});
function update<K extends keyof typeof draft.value>(field: K, value: (typeof draft.value)[K]) {
  if (!loading.value && !disposed) draft.value[field] = value;
}
async function submit() {
  if (disposed || loading.value || !form.value) return;
  loading.value = true;
  try {
    await form.value.validateFields();
    if (disposed || !draft.value.protocol) return;
    const values: CreateSiteInput = { ...draft.value, protocol: draft.value.protocol };
    const siteId = await createSite(values);
    await store.loadSite({ autoLoadDetails: false });
    if (disposed) return;
    if (store.sites.some((item) => item.id === siteId)) {
      store.selectedSiteId = siteId;
      void detailsStore.loadSiteDetails();
    }
    emit('close');
  } catch {
    // 校验提示由表单展示，接口错误由 API 提示；失败时保留表单，不刷新列表。
  } finally {
    loading.value = false;
  }
}
</script>
<template>
  <a-modal
    open
    title="添加站点"
    :width="560"
    :confirm-loading="loading"
    :closable="!loading"
    :keyboard="!loading"
    :cancel-button-props="{ disabled: loading }"
    centered
    :mask="{ closable: false }"
    :after-open-change="(open: boolean) => open && nameInput?.focus()"
    ok-text="连接并获取模型"
    cancel-text="取消"
    @cancel="!loading && emit('close')"
    @ok="submit"
  >
    <a-form
      ref="form"
      :model="draft"
      :disabled="loading"
      layout="vertical"
      :validate-trigger="['change', 'blur']"
      class="pt-2"
    >
      <a-form-item
        label="站点名称"
        name="name"
        :rules="[{ required: true, whitespace: true, message: '请输入站点名称' }]"
      >
        <a-input
          ref="nameInput"
          :value="draft.name"
          autocomplete="off"
          placeholder="例如：OpenAI 主线路"
          @update:value="update('name', $event)"
        />
      </a-form-item>
      <a-form-item
        label="接口协议"
        name="protocol"
        :rules="[{ required: true, message: '请选择接口协议' }]"
      >
        <a-select
          :value="draft.protocol"
          :options="protocolOptions"
          placeholder="请选择接口协议"
          not-found-content="暂无可用协议"
          @update:value="update('protocol', $event)"
        />
      </a-form-item>
      <a-form-item
        label="Base URL"
        name="baseUrl"
        :rules="[
          { required: true, whitespace: true, message: '请输入 Base URL' },
          { type: 'url', message: '请输入有效的 Base URL' },
        ]"
      >
        <a-input
          :value="draft.baseUrl"
          autocomplete="off"
          placeholder="https://api.example.com/v1"
          @update:value="update('baseUrl', $event)"
        />
      </a-form-item>
      <a-form-item
        label="API Key"
        name="secret"
        :rules="[{ required: true, whitespace: true, message: '请输入访问凭据' }]"
      >
        <a-input-password
          :value="draft.secret"
          autocomplete="new-password"
          placeholder="输入访问凭据"
          @update:value="update('secret', $event)"
        />
      </a-form-item>
      <a-form-item class="mb-0!">
        <a-checkbox :checked="draft.enabled" @update:checked="update('enabled', $event as boolean)"
          >在小面板中显示</a-checkbox
        >
      </a-form-item>
    </a-form>
  </a-modal>
</template>
