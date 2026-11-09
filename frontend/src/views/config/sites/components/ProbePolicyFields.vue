<script setup lang="ts">
import type { CustomProbeRequest, ProbePolicy, SiteProtocol } from '@/types/domain/sites';
import type { RequestParameterDraft } from '@/types/request-parameters';

import { computed, reactive, watch } from 'vue';

import {
  buildRequestParametersPreview,
  createRequestParameterDrafts,
  requestParameterValidation,
  serializeRequestParameters,
} from '@/utils/request-parameters';

import { customEndpointError, resolveCustomProbeEndpoint } from '../probe-policy';

import RequestParameterEditor from '@/components/request-parameter/editor.vue';
import RequestParameterPreview from '@/components/request-parameter/preview.vue';

const policy = defineModel<ProbePolicy>({ required: true });
const customRequest = defineModel<CustomProbeRequest>('customRequest');
const props = defineProps<{
  protocol: SiteProtocol;
  modelName?: string;
  baseUrl?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ change: [] }>();
const previewModel = computed(() => props.modelName ?? '<模型名称>');

// 两套请求各自保留草稿，重新打开时从已保存的完整参数表载入。
const drafts = new WeakMap<object, RequestParameterDraft[]>();
const parameters = computed(() => {
  const source = customRequest.value ?? policy.value;
  let draft = drafts.get(source);
  if (!draft) {
    draft = reactive(createRequestParameterDrafts(source.parameters));
    drafts.set(source, draft);
  }
  return draft;
});
const endpointValidation = computed(() =>
  customRequest.value
    ? customEndpointError(props.baseUrl ?? '', customRequest.value.endpoint)
    : null,
);
const parameterValidation = computed(() => requestParameterValidation(parameters.value));
const parameterError = computed(() =>
  parameterValidation.value.flatMap((item) => [item.key, item.value]).find(Boolean),
);
const error = computed(() => endpointValidation.value ?? parameterError.value);

watch(
  parameters,
  (value) => {
    const source = customRequest.value ?? policy.value;
    source.parameters = serializeRequestParameters(value);
    emit('change');
  },
  { deep: true, flush: 'sync' },
);
const canSubmit = computed(() => !error.value);
const submissionHint = computed(() => error.value ?? undefined);

const preview = computed(() => {
  if (error.value) return '请先修正配置后查看预览。';
  const body = JSON.stringify(
    buildRequestParametersPreview(parameters.value, previewModel.value),
    null,
    2,
  );
  return customRequest.value
    ? `POST ${resolveCustomProbeEndpoint(props.baseUrl ?? '', customRequest.value.endpoint)}\n\n${body}`
    : body;
});

function validate() {
  return canSubmit.value ? Promise.resolve() : Promise.reject(new Error(submissionHint.value));
}
defineExpose({ validate, canSubmit, submissionHint });
</script>
<template>
  <a-form :model="policy" layout="vertical" :disabled="disabled" :required-mark="false">
    <section v-if="customRequest" class="policy-section">
      <a-form-item
        label="自定义探测端点"
        class="mb-2!"
        :validate-status="endpointValidation ? 'error' : undefined"
        :help="endpointValidation ?? undefined"
      >
        <a-input
          :value="customRequest.endpoint"
          aria-label="自定义探测端点"
          placeholder="例如 /v1/images/generations"
          @update:value="!disabled && (customRequest.endpoint = $event)"
        />
      </a-form-item>
      <div class="flex flex-wrap gap-1">
        <a-tag>POST</a-tag>
        <a-tag color="blue">继承站点凭据和请求头</a-tag>
        <a-tag>首 Token 显示 —</a-tag>
      </div>
    </section>
    <RequestParameterEditor
      :parameters="parameters"
      :disabled="disabled"
      :model-name="modelName"
      class="policy-section"
    />
    <RequestParameterPreview :value="preview" class="policy-section">
      <template #extra><slot name="preview-extra" /></template>
      <slot name="preview-result" />
    </RequestParameterPreview>
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
