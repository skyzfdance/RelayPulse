<script setup lang="ts">
import type { RequestParameterDraft } from '../request-parameters';
import type { CustomProbeRequest, ProbePolicy, SiteProtocol } from '@/types/domain/sites';

import { computed, reactive, watch } from 'vue';

import { customEndpointError, resolveCustomProbeEndpoint } from '../probe-policy';
import {
  buildRequestParametersPreview,
  createRequestParameterDrafts,
  newRequestParameter,
  parameterBlocked,
  parameterEnabled,
  removeRequestParameter,
  requestParameterRows,
  requestParameterValidation,
  serializeRequestParameters,
  updateRequestParameterKey,
} from '../request-parameters';

import AppIcon from '@/components/AppIcon.vue';

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
const rows = computed(() => requestParameterRows(parameters.value));
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
const enabledCount = computed(
  () =>
    parameters.value.filter((parameter) => parameterEnabled(parameters.value, parameter)).length,
);
const parameterTypes = ['string', 'number', 'boolean', 'object', 'json'].map((value) => ({
  value,
  label: value === 'json' ? 'JSON' : value,
}));
const booleanValues = ['true', 'false'].map((value) => ({ value, label: value }));

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

function rowValidation(parameter: RequestParameterDraft) {
  return parameterValidation.value[parameters.value.indexOf(parameter)];
}
function hasChildren(parameter: RequestParameterDraft) {
  return Boolean(
    parameter.key && parameters.value.some((child) => child.key.startsWith(`${parameter.key}.`)),
  );
}
function rowDisabled(parameter: RequestParameterDraft) {
  return props.disabled || !parameterEnabled(parameters.value, parameter);
}
function setEnabled(parameter: RequestParameterDraft, enabled: unknown) {
  if (!props.disabled && !parameterBlocked(parameters.value, parameter))
    parameter.enabled = Boolean(enabled);
}
function updateParameter<K extends 'key' | 'value' | 'type'>(
  parameter: RequestParameterDraft,
  field: K,
  value: RequestParameterDraft[K],
) {
  if (rowDisabled(parameter) || parameter.binding) return;
  if (field === 'key') updateRequestParameterKey(parameters.value, parameter, value);
  else if (field === 'type') {
    if (hasChildren(parameter) && value !== 'object') return;
    parameter.type = value as RequestParameterDraft['type'];
    if (value === 'object') parameter.value = '{}';
  } else parameter.value = value;
}
function addParameter(parent?: RequestParameterDraft) {
  if (props.disabled || (parent && rowDisabled(parent))) return;
  const parameter = newRequestParameter(parent ? `${parent.key}.` : '');
  if (parent) parameters.value.splice(parameters.value.indexOf(parent) + 1, 0, parameter);
  else parameters.value.push(parameter);
}
function removeParameter(parameter: RequestParameterDraft) {
  if (!props.disabled && !parameterBlocked(parameters.value, parameter))
    removeRequestParameter(parameters.value, parameter);
}
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
    <section class="policy-section">
      <div class="mb-3 flex items-center justify-between gap-4">
        <div>
          <h3>请求参数</h3>
          <p class="policy-description">勾选的参数加入请求，取消勾选保留配置</p>
        </div>
        <a-button :disabled="disabled" @click="addParameter()">
          <template #icon><AppIcon name="PlusOutlined" /></template>添加参数
        </a-button>
      </div>
      <div class="overflow-x-auto rounded-md border border-rp-line">
        <div class="parameter-table" role="table" aria-label="请求参数">
          <div
            class="parameter-row parameter-header min-h-9.5 border-b border-rp-line bg-rp-subtle px-2.5 text-xs text-rp-secondary"
            role="row"
          >
            <span role="columnheader" class="text-center">启用</span>
            <span role="columnheader">字段</span>
            <span role="columnheader">值</span>
            <span role="columnheader">类型</span>
            <span role="columnheader" class="sr-only">操作</span>
          </div>
          <div
            v-for="({ parameter, depth, blocked }, index) in rows"
            :key="parameter.id"
            :data-parameter-key="parameter.key"
            :data-parameter-enabled="parameterEnabled(parameters, parameter)"
            class="parameter-row min-h-12 border-b border-rp-line px-2.5 py-2 last:border-b-0"
            :class="{ 'parameter-inactive': !parameterEnabled(parameters, parameter) }"
            role="row"
          >
            <div role="cell" class="flex items-center justify-center">
              <a-checkbox
                :checked="parameter.enabled"
                :disabled="disabled || blocked"
                :aria-label="`启用参数 ${parameter.key || index + 1}`"
                @update:checked="setEnabled(parameter, $event)"
              />
            </div>
            <div
              role="cell"
              class="flex min-w-0 items-center"
              :style="{ paddingLeft: `${depth * 14}px` }"
            >
              <span v-if="depth" class="parameter-branch mr-1.5 shrink-0" aria-hidden="true" />
              <a-input
                :value="parameter.key"
                :disabled="rowDisabled(parameter) || Boolean(parameter.binding)"
                :status="rowValidation(parameter)?.key ? 'error' : undefined"
                :aria-label="`参数字段 ${index + 1}`"
                placeholder="参数名称"
                class="font-mono"
                @update:value="updateParameter(parameter, 'key', $event)"
              />
            </div>
            <div role="cell" class="min-w-0">
              <div
                v-if="parameter.binding"
                class="flex min-h-8 items-center gap-1.5 text-xs text-rp-secondary"
                :class="{ 'text-rp-tertiary': rowDisabled(parameter) }"
              >
                <AppIcon name="LinkOutlined" />
                <span>{{ props.modelName || '当前探测模型' }}</span>
              </div>
              <div
                v-else-if="parameter.type === 'object'"
                class="flex min-h-8 items-center font-mono text-xs text-rp-tertiary"
              >
                {}
              </div>
              <a-select
                v-else-if="parameter.type === 'boolean'"
                :value="parameter.value"
                :options="booleanValues"
                :disabled="rowDisabled(parameter)"
                :status="rowValidation(parameter)?.value ? 'error' : undefined"
                :aria-label="`参数值 ${index + 1}`"
                class="w-full font-mono"
                @update:value="updateParameter(parameter, 'value', $event)"
              />
              <a-textarea
                v-else-if="parameter.type === 'json'"
                :value="parameter.value"
                :auto-size="{ minRows: 1, maxRows: 5 }"
                :disabled="rowDisabled(parameter)"
                :status="rowValidation(parameter)?.value ? 'error' : undefined"
                :aria-label="`参数值 ${index + 1}`"
                :spellcheck="false"
                class="font-mono text-xs!"
                @update:value="updateParameter(parameter, 'value', $event)"
              />
              <a-input
                v-else
                :value="parameter.value"
                :disabled="rowDisabled(parameter)"
                :status="rowValidation(parameter)?.value ? 'error' : undefined"
                :aria-label="`参数值 ${index + 1}`"
                class="font-mono"
                @update:value="updateParameter(parameter, 'value', $event)"
              />
            </div>
            <div role="cell">
              <a-select
                :value="parameter.type"
                :disabled="rowDisabled(parameter) || Boolean(parameter.binding)"
                :options="
                  hasChildren(parameter)
                    ? parameterTypes.filter((type) => type.value === 'object')
                    : parameterTypes
                "
                :aria-label="`参数类型 ${index + 1}`"
                class="w-full"
                @update:value="updateParameter(parameter, 'type', $event)"
              />
            </div>
            <div role="cell" class="flex items-center justify-end">
              <a-button
                v-if="parameter.type === 'object'"
                type="text"
                size="small"
                :disabled="rowDisabled(parameter)"
                :aria-label="`添加 ${parameter.key || '对象'} 的子参数`"
                title="添加子参数"
                @click="addParameter(parameter)"
              >
                <template #icon><AppIcon name="PlusOutlined" /></template>
              </a-button>
              <a-button
                type="text"
                size="small"
                :disabled="disabled || blocked"
                :aria-label="`删除参数 ${index + 1}`"
                :title="hasChildren(parameter) ? '删除参数及其子参数' : '删除参数'"
                @click="removeParameter(parameter)"
              >
                <template #icon><AppIcon name="DeleteOutlined" /></template>
              </a-button>
            </div>
          </div>
          <div v-if="!rows.length" class="px-3 py-5 text-center text-xs text-rp-tertiary">
            暂无请求参数，可添加参数配置请求
          </div>
        </div>
      </div>
      <div class="mt-2 flex items-start justify-between gap-3 text-xs">
        <p
          class="m-0!"
          :class="parameterError ? 'text-rp-error' : 'text-rp-tertiary'"
          :role="parameterError ? 'alert' : undefined"
        >
          {{ parameterError ?? '支持对象路径；关闭父级时子参数不发送，空对象仍可发送。' }}
        </p>
        <span class="shrink-0 text-rp-tertiary"
          >已启用 {{ enabledCount }} / {{ parameters.length }}</span
        >
      </div>
    </section>
    <section class="policy-section">
      <div class="flex items-center justify-between gap-4">
        <div>
          <h3>有效请求预览</h3>
          <p class="policy-description">按当前启用的参数生成请求</p>
        </div>
        <slot name="preview-extra" />
      </div>
      <pre
        aria-label="有效请求预览"
        class="mt-3! mb-0! overflow-x-auto rounded-md bg-rp-surface p-3 font-mono text-[11px]! leading-4! whitespace-pre-wrap wrap-anywhere"
        >{{ preview }}</pre>
      <slot name="preview-result" />
    </section>
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
.policy-section h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 22px;
}
.policy-description {
  margin: 0;
  font-size: 12px;
  line-height: 20px;
  color: var(--ant-color-text-tertiary);
}
.parameter-table {
  min-width: 580px;
}
.parameter-row {
  display: grid;
  grid-template-columns: 38px minmax(0, 1.2fr) minmax(0, 1.1fr) 110px 48px;
  align-items: start;
  gap: 10px;
}
.parameter-row > :first-child {
  min-height: 32px;
}
.parameter-row > :last-child {
  padding-top: 4px;
}
.parameter-header {
  align-items: center;
}
.parameter-header > :first-child {
  min-height: auto;
}
.parameter-branch {
  width: 7px;
  height: 10px;
  border-bottom: 1px solid var(--ant-color-border);
  border-left: 1px solid var(--ant-color-border);
}
.parameter-inactive {
  background: var(--ant-color-fill-quaternary);
}
</style>
