<script setup lang="ts">
import type { ManagedSite } from '@/types/domain/sites';
import type { RequestParameterDraft } from '@/types/request-parameters';

import { computed, ref, watch } from 'vue';

import { fetchSite } from '@/api/config/sites';
import { useIqTestStore } from '@/stores/iq-test';
import {
  createRequestParameterDrafts,
  parameterEnabled,
  serializeRequestParameters,
} from '@/utils/request-parameters';
import { requestUrl } from '@/utils/request-url';
import {
  siteHeadersError,
  siteHeaderValidation,
  siteRequestPreview,
} from '@/views/config/sites/site-request';

import { buildIqParameters, iqParameterError, iqPresets } from '../iq-parameters';

import RequestEditor from '@/components/request-parameter/request-editor.vue';

const open = defineModel<boolean>('open', { default: false });
const store = useIqTestStore();
const modelKey = ref<string>();
const preset = ref('high');
const parameters = ref<RequestParameterDraft[]>([]);
const headers = ref<RequestParameterDraft[]>([]);
const sourceSite = ref<ManagedSite>();
const sourceError = ref('');
const sourceLoading = ref(false);
const copied = ref(false);
let sourceRevision = 0;
const saving = ref(false);
const candidate = computed(() => store.candidates.find((item) => item.modelKey === modelKey.value));
const presets = iqPresets();
const groups = computed(() => {
  const sites = new Map<string, { label: string; options: { label: string; value: string }[] }>();
  for (const item of store.candidates) {
    if (!sites.has(item.siteId)) sites.set(item.siteId, { label: item.siteName, options: [] });
    sites.get(item.siteId)!.options.push({ label: item.modelName, value: item.modelKey });
  }
  return [...sites.values()];
});
const error = computed(() => siteHeadersError(headers.value) ?? iqParameterError(parameters.value));
const preview = computed(() => {
  if (!candidate.value) return '请先选择模型后查看预览。';
  if (error.value) return '请先修正配置后查看预览。';
  if (!sourceSite.value) return sourceError.value || '正在读取请求地址…';
  try {
    const body = buildIqParameters(candidate.value.protocol, preset.value, parameters.value);
    Object.assign(body, { model: candidate.value.modelName, stream: true });
    if (candidate.value.protocol === 'openai-responses')
      body.input = [{ role: 'user', content: '<当前题目，由测试流程填入>' }];
    else body.messages = [{ role: 'user', content: '<当前题目，由测试流程填入>' }];
    return siteRequestPreview(
      requestUrl(sourceSite.value.baseUrl, sourceSite.value.connection.probePath),
      candidate.value.protocol,
      headers.value,
      body,
    );
  } catch (cause) {
    return cause instanceof Error ? cause.message : '请求配置无效';
  }
});
watch(open, (value) => {
  if (!value) return;
  modelKey.value = undefined;
  parameters.value = [];
  headers.value = [];
  sourceSite.value = undefined;
  sourceError.value = '';
  copied.value = false;
  preset.value = 'high';
  void store.refresh();
});
watch(
  () => candidate.value?.siteId,
  async (siteId) => {
    const revision = ++sourceRevision;
    sourceSite.value = undefined;
    sourceError.value = '';
    if (!siteId) {
      sourceLoading.value = false;
      return;
    }
    sourceLoading.value = true;
    try {
      const site = await fetchSite(siteId);
      if (revision === sourceRevision) sourceSite.value = site;
    } catch {
      if (revision === sourceRevision) sourceError.value = '源站点读取失败，请重新选择模型';
    } finally {
      if (revision === sourceRevision) sourceLoading.value = false;
    }
  },
);
function copyHeaders() {
  if (!sourceSite.value || !candidate.value || saving.value) return;
  const modelId = candidate.value.modelKey.slice(candidate.value.siteId.length + 1);
  const model = sourceSite.value.models.find((item) => item.id === modelId);
  const request = model?.customRequest ?? model?.policy ?? sourceSite.value.policy;
  headers.value = createRequestParameterDrafts(request.headers);
  copied.value = true;
}
async function submit() {
  if (!candidate.value || error.value || saving.value) return;
  saving.value = true;
  try {
    const count = parameters.value.filter((p) => parameterEnabled(parameters.value, p)).length;
    await store.addParticipant({
      modelKey: candidate.value.modelKey,
      headers: serializeRequestParameters(headers.value),
      preset:
        preset.value === 'custom'
          ? `自定义 · ${count} 项参数`
          : presets.find((p) => p.value === preset.value)!.label,
      parameters: buildIqParameters(candidate.value.protocol, preset.value, parameters.value),
    });
    open.value = false;
  } catch {
    // 具名 API 统一显示失败，保留草稿。
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <a-drawer
    v-model:open="open"
    :size="620"
    :closable="{ placement: 'start', disabled: saving, 'aria-label': '关闭参与者配置' }"
    :keyboard="!saving"
    :mask-closable="!saving"
    :styles="{
      wrapper: { maxWidth: '100vw' },
      header: { padding: '12px 20px' },
      body: { padding: '16px 20px' },
    }"
  >
    <template #title>
      <div>
        <strong>添加参与者</strong>
        <div class="mt-0.5 text-xs font-normal text-rp-tertiary">
          选择模型并独立配置 IQ 参数，添加后自动检测当前快照。
        </div>
      </div>
    </template>
    <a-form :model="{ modelKey, preset }" layout="vertical" :disabled="saving">
      <a-form-item label="模型" required>
        <a-select
          v-model:value="modelKey"
          aria-label="参与者模型"
          show-search
          option-filter-prop="label"
          :options="groups"
          class="w-full"
          placeholder="按站点选择启用的模型"
        />
        <div v-if="!store.candidates.length" class="mt-2 text-xs text-rp-secondary">
          暂无候选模型，<RouterLink to="/config/sites" @click="open = false"
            >前往站点管理启用模型</RouterLink
          >
        </div>
      </a-form-item>
    </a-form>
    <RequestEditor
      v-model:parameters="parameters"
      v-model:headers="headers"
      :disabled="saving"
      :header-validation="siteHeaderValidation"
      :error="iqParameterError(parameters)"
      :preview="preview"
      preview-description="题面由测试流程填入；认证信息已脱敏。"
      body-description="手写注入字段，优先级高于预设档位"
      body-hint="支持对象路径；关闭父级时子参数不发送。题面、model、stream 为系统保留字段。"
    >
      <template #headers-before>
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <span class="text-xs text-rp-secondary">{{
            copied ? '已从模型复制，当前为独立副本' : '本参与者独立配置'
          }}</span>
          <a-button :disabled="saving || !sourceSite || sourceLoading" @click="copyHeaders"
            >从模型复制请求头</a-button
          >
        </div>
        <p v-if="sourceError" class="text-rp-error" role="alert">{{ sourceError }}</p>
      </template>
      <template #body-before>
        <a-form layout="vertical" :disabled="saving"
          ><a-form-item label="推理强度">
            <a-select
              v-model:value="preset"
              aria-label="推理强度"
              :options="presets"
              :disabled="!candidate"
              class="w-full"
            /> </a-form-item
        ></a-form>
      </template>
    </RequestEditor>
    <template #footer
      ><div class="flex justify-end gap-2">
        <a-button :disabled="saving" @click="open = false">取消</a-button
        ><a-button
          type="primary"
          :disabled="!candidate || Boolean(error)"
          :loading="saving"
          @click="submit"
          >添加参与者</a-button
        >
      </div></template
    >
  </a-drawer>
</template>
