<script setup lang="ts">
import type { ProbePolicy, SiteProtocol } from '@/types/domain/sites';
import type { RequestParameterDraft } from '@/types/request-parameters';

import { computed, reactive, watch } from 'vue';

import {
  createRequestParameterDrafts,
  requestParameterValidation,
  serializeRequestParameters,
} from '@/utils/request-parameters';

import { customEndpointError } from '../probe-policy';
import { buildCustomProbePreview, buildProbePreview } from '../probe-preview';
import { siteHeadersError, siteHeaderValidation, siteRequestPreview } from '../site-request';

import RequestEditor from '@/components/request-parameter/request-editor.vue';

const policy = defineModel<ProbePolicy>({ required: true });
const customEndpoint = defineModel<string>('customEndpoint');
const props = defineProps<{
  protocol: SiteProtocol;
  modelName?: string;
  baseUrl?: string;
  probePath?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ change: [] }>();
const previewModel = computed(() => props.modelName ?? '<模型名称>');

// 参数只跟随策略草稿，是否自定义端点不影响 Headers / Body。
const drafts = new WeakMap<object, RequestParameterDraft[]>();
const parameters = computed(() => {
  const source = policy.value;
  let draft = drafts.get(source);
  if (!draft) {
    draft = reactive(createRequestParameterDrafts(source.parameters));
    drafts.set(source, draft);
  }
  return draft;
});
const headerDrafts = new WeakMap<object, RequestParameterDraft[]>();
const headers = computed(() => {
  const source = policy.value;
  let draft = headerDrafts.get(source);
  if (!draft) {
    draft = reactive(createRequestParameterDrafts(source.headers));
    headerDrafts.set(source, draft);
  }
  return draft;
});
const editableParameters = computed({
  get: () => parameters.value,
  set: (value) => parameters.value.splice(0, parameters.value.length, ...value),
});
const editableHeaders = computed({
  get: () => headers.value,
  set: (value) => headers.value.splice(0, headers.value.length, ...value),
});
const endpointValidation = computed(() =>
  customEndpoint.value !== undefined
    ? customEndpointError(props.baseUrl ?? '', customEndpoint.value)
    : null,
);
const parameterValidation = computed(() => requestParameterValidation(parameters.value));
const parameterError = computed(() =>
  parameterValidation.value.flatMap((item) => [item.key, item.value]).find(Boolean),
);
const error = computed(
  () => endpointValidation.value ?? siteHeadersError(headers.value) ?? parameterError.value,
);

watch(
  parameters,
  (value) => {
    policy.value.parameters = serializeRequestParameters(value);
    emit('change');
  },
  { deep: true, flush: 'sync' },
);
watch(
  headers,
  (value) => {
    policy.value.headers = serializeRequestParameters(value);
    emit('change');
  },
  { deep: true, flush: 'sync' },
);
const canSubmit = computed(() => !error.value);
const submissionHint = computed(() => error.value ?? undefined);

const preview = computed(() => {
  if (error.value) return '请先修正配置后查看预览。';
  const current = { ...policy.value, headers: headers.value, parameters: parameters.value };
  const request =
    customEndpoint.value !== undefined
      ? buildCustomProbePreview(props.baseUrl ?? '', previewModel.value, {
          ...current,
          endpoint: customEndpoint.value,
        })
      : {
          endpoint: `${props.baseUrl ?? ''}${props.probePath ?? ''}`,
          body: buildProbePreview(previewModel.value, props.protocol, current),
        };
  return siteRequestPreview(request.endpoint, props.protocol, headers.value, request.body);
});

function validate() {
  return canSubmit.value ? Promise.resolve() : Promise.reject(new Error(submissionHint.value));
}
defineExpose({ validate, canSubmit, submissionHint });
</script>
<template>
  <a-form :model="policy" layout="vertical" :disabled="disabled" :required-mark="false">
    <section v-if="customEndpoint !== undefined" class="policy-section">
      <a-form-item
        label="自定义探测端点"
        class="mb-2!"
        :validate-status="endpointValidation ? 'error' : undefined"
        :help="endpointValidation ?? undefined"
      >
        <a-input
          :value="customEndpoint"
          aria-label="自定义探测端点"
          placeholder="按实际请求端点填写"
          @update:value="!disabled && (customEndpoint = $event)"
        />
      </a-form-item>
      <div class="flex flex-wrap gap-1">
        <a-tag>POST</a-tag>
        <a-tag color="blue">使用站点凭据</a-tag>
        <a-tag v-if="protocol === 'custom-http'">首 Token 显示 —</a-tag>
      </div>
    </section>
    <RequestEditor
      v-model:parameters="editableParameters"
      v-model:headers="editableHeaders"
      :disabled="disabled"
      :header-validation="siteHeaderValidation"
      :model-name="modelName"
      :preview="preview"
      :error="parameterError ?? undefined"
      class="policy-section"
    >
      <template #preview-extra><slot name="preview-extra" /></template>
      <template #preview-result><slot name="preview-result" /></template>
    </RequestEditor>
  </a-form>
</template>
<style scoped>
.policy-section {
  border-bottom: 1px solid var(--ant-color-border-secondary);
  padding-bottom: 16px;
}
.policy-section + .policy-section {
  padding-top: 16px;
}
</style>
