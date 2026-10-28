<script setup lang="ts">
import type { SiteDraft, SiteProtocolOption } from '@/types/domain/sites';

import { computed, onMounted, ref } from 'vue';

import { fetchSiteProtocols } from '@/api/catalog';

const protocolOptions = ref<SiteProtocolOption[]>([]);
onMounted(async () => {
  try {
    protocolOptions.value = await fetchSiteProtocols();
  } catch {
    // 接口已提示；保留当前表单。
  }
});
const draft = defineModel<SiteDraft>({ required: true });
const props = defineProps<{
  credentialConfigured?: boolean;
  disabled?: boolean;
}>();
const credentialPlaceholder = computed(() =>
  props.credentialConfigured ? '留空则保留当前凭据' : '输入访问凭据',
);
const credentialHint = computed(() => (props.credentialConfigured ? '已配置凭据' : '尚未配置凭据'));
function update<K extends keyof SiteDraft>(field: K, value: SiteDraft[K]) {
  if (props.disabled) return;
  draft.value[field] = value;
  if (field === 'secret') draft.value.credentialAction = value ? 'replace' : 'keep';
}
</script>
<template>
  <a-form :model="draft" layout="vertical" :required-mark="false" :disabled="disabled">
    <div class="grid grid-cols-2 gap-x-5">
      <a-form-item label="站点名称" name="name" required>
        <a-input
          :value="draft.name"
          autocomplete="off"
          placeholder="例如：主线路"
          @update:value="update('name', $event)"
        />
      </a-form-item>
      <a-form-item label="接口协议" name="protocol">
        <a-select
          :value="draft.protocol"
          :options="protocolOptions"
          @update:value="update('protocol', $event)"
        />
      </a-form-item>
      <a-form-item label="Base URL" name="baseUrl" required>
        <a-input
          :value="draft.baseUrl"
          autocomplete="off"
          placeholder="https://api.example.com"
          @update:value="update('baseUrl', $event)"
        />
      </a-form-item>
      <a-form-item label="访问凭据" name="secret" :extra="credentialHint">
        <a-input-password
          :value="draft.secret"
          autocomplete="new-password"
          :disabled="disabled"
          :placeholder="credentialPlaceholder"
          @update:value="update('secret', $event)"
        />
      </a-form-item>
    </div>
  </a-form>
</template>
