<script setup lang="ts">
import type { CreateSiteInput, SiteProtocol, SiteProtocolOption } from '@/types/domain/sites';
import type { FormInstance, InputRef } from 'antdv-next';

import { computed, onMounted, onScopeDispose, ref } from 'vue';

import { fetchSiteProtocols } from '@/api/catalog';
import { createSite } from '@/api/config/sites';
import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { useFeedback } from '@/composables/useFeedback';
import { useSiteDetailsStore, useSiteStore } from '@/stores/site';
import { requestUrl } from '@/utils/request-url';

const emit = defineEmits<{ close: [] }>();
const store = useSiteStore();
const detailsStore = useSiteDetailsStore();
const feedback = useFeedback();
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
  modelsPath: '',
  probePath: '',
  secret: '',
  enabled: false,
});
const probePaths: Record<SiteProtocol, string> = {
  'openai-compatible': '/chat/completions',
  'openai-responses': '/responses',
  anthropic: '/messages',
  'custom-http': '',
};
const probeHint = computed(() => {
  const path = draft.value.protocol && probePaths[draft.value.protocol];
  return path ? `例如 ${path}` : '按服务文档填写';
});
function selectProtocol(protocol?: SiteProtocol) {
  if (loading.value || disposed || draft.value.protocol === protocol) return;
  draft.value.protocol = protocol;
  draft.value.modelsPath = protocol && protocol !== 'custom-http' ? '/models' : '';
  draft.value.probePath = protocol ? probePaths[protocol] : '';
}
function addressPreview(endpoint: string): string {
  if (!endpoint) return '';
  try {
    return requestUrl(draft.value.baseUrl, endpoint);
  } catch {
    return '';
  }
}
onMounted(async () => {
  try {
    const options = await fetchSiteProtocols();
    if (disposed) return;
    protocolOptions.value = options;
    selectProtocol(options[0]?.value);
  } catch (cause) {
    if (!disposed) reportOperationReadFailure('site_list', 'page', cause);
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
    const created = store.sites.find((item) => item.id === siteId);
    if (created) {
      store.selectedSiteId = siteId;
      void detailsStore.loadSiteDetails();
    }
    if (!values.modelsPath) {
      feedback.success('站点已添加，可手动添加模型');
    } else if (created?.modelCount === 0) {
      feedback.warning(
        '站点已添加，未获取到模型。请在高级设置中调整模型列表端点后同步模型，也可手动添加模型。',
      );
    } else {
      feedback.success('站点已添加');
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
    ok-text="添加站点"
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
          @update:value="selectProtocol($event)"
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
          placeholder="例如 https://api.example.com/v1；按实际地址填写"
          @update:value="update('baseUrl', $event)"
        />
      </a-form-item>
      <div class="grid grid-cols-2 gap-x-4">
        <a-form-item label="模型列表端点（可选）" name="modelsPath">
          <a-input
            :value="draft.modelsPath"
            :placeholder="
              draft.protocol === 'custom-http' ? '按服务文档填写，可留空' : '例如 /models；可留空'
            "
            @update:value="update('modelsPath', $event)"
          />
          <template #extra>
            <span v-if="!draft.modelsPath">留空则跳过获取模型</span>
            <code v-else class="break-all">{{ addressPreview(draft.modelsPath) }}</code>
          </template>
        </a-form-item>
        <a-form-item
          label="探测端点"
          name="probePath"
          :rules="[{ required: true, whitespace: true, message: '请输入探测端点' }]"
        >
          <a-input
            :value="draft.probePath"
            :placeholder="probeHint"
            @update:value="update('probePath', $event)"
          />
          <template #extra
            ><code class="break-all">{{ addressPreview(draft.probePath) }}</code></template
          >
        </a-form-item>
      </div>
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
