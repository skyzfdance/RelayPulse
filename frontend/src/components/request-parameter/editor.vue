<script setup lang="ts">
import type { RequestParameterDraft } from '@/types/request-parameters';

import { computed, ref } from 'vue';

import {
  newRequestParameter,
  parameterBlocked,
  parameterEnabled,
  removeRequestParameter,
  requestParameterRows,
  requestParameterValidation,
  updateRequestParameterKey,
} from '@/utils/request-parameters';

import AppIcon from '@/components/AppIcon.vue';
import JsonParameterImport from '@/components/request-parameter/json-import.vue';

defineOptions({ name: 'RequestParameterEditor' });

const parameters = defineModel<RequestParameterDraft[]>('parameters', { required: true });
const importOpen = ref(false);
const props = withDefaults(
  defineProps<{
    disabled?: boolean;
    modelName?: string;
    description?: string;
    hint?: string;
    emptyText?: string;
    error?: string;
  }>(),
  {
    modelName: '',
    error: '',
    description: '勾选的参数加入请求，取消勾选保留配置',
    hint: '支持对象路径；关闭父级时子参数不发送，空对象仍可发送。',
    emptyText: '暂无请求参数，可添加参数配置请求',
  },
);
const rows = computed(() => requestParameterRows(parameters.value));
const validation = computed(() => requestParameterValidation(parameters.value));
const validationError = computed(() =>
  validation.value.flatMap((item) => [item.key, item.value]).find(Boolean),
);
const error = computed(() => props.error || validationError.value);
const enabledCount = computed(
  () =>
    parameters.value.filter((parameter) => parameterEnabled(parameters.value, parameter)).length,
);
const parameterTypes = ['string', 'number', 'boolean', 'object', 'json'].map((value) => ({
  value,
  label: value === 'json' ? 'JSON' : value,
}));
const booleanValues = ['true', 'false'].map((value) => ({ value, label: value }));

function rowValidation(parameter: RequestParameterDraft) {
  return validation.value[parameters.value.indexOf(parameter)];
}
function hasChildren(parameter: RequestParameterDraft) {
  return Boolean(
    parameter.key && parameters.value.some((child) => child.key.startsWith(`${parameter.key}.`)),
  );
}
function rowDisabled(parameter: RequestParameterDraft) {
  return props.disabled || !parameterEnabled(parameters.value, parameter);
}
function setEnabled(parameter: RequestParameterDraft, checked: unknown) {
  if (!props.disabled && !parameterBlocked(parameters.value, parameter))
    parameter.enabled = Boolean(checked);
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
function openJsonImport() {
  if (!props.disabled) importOpen.value = true;
}
function applyImportedParameters(imported: RequestParameterDraft[]) {
  if (!props.disabled) parameters.value.splice(0, parameters.value.length, ...imported);
}
</script>

<template>
  <section class="request-parameter-section">
    <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
      <div>
        <h3>请求参数</h3>
        <p class="request-parameter-description">{{ description }}</p>
      </div>
      <div class="flex shrink-0 flex-wrap justify-end gap-2">
        <a-button :disabled="disabled" @click="openJsonImport">
          <template #icon><AppIcon name="CodeOutlined" /></template>粘贴 JSON
        </a-button>
        <a-button :disabled="disabled" @click="addParameter()">
          <template #icon><AppIcon name="PlusOutlined" /></template>添加参数
        </a-button>
      </div>
    </div>
    <div class="overflow-x-auto rounded-md border border-rp-line">
      <div class="request-parameter-table" role="table" aria-label="请求参数">
        <div
          class="request-parameter-row request-parameter-header min-h-9.5 border-b border-rp-line bg-rp-subtle px-2.5 text-xs text-rp-secondary"
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
          class="request-parameter-row min-h-12 border-b border-rp-line px-2.5 py-2 last:border-b-0"
          :class="{ 'request-parameter-inactive': !parameterEnabled(parameters, parameter) }"
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
            <span
              v-if="depth"
              class="request-parameter-branch mr-1.5 shrink-0"
              aria-hidden="true"
            />
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
              <span>{{ modelName || '当前探测模型' }}</span>
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
          {{ emptyText }}
        </div>
      </div>
    </div>
    <div class="mt-2 flex items-start justify-between gap-3 text-xs">
      <p
        class="m-0!"
        :class="error ? 'text-rp-error' : 'text-rp-tertiary'"
        :role="error ? 'alert' : undefined"
      >
        {{ error ?? hint }}
      </p>
      <span class="shrink-0 text-rp-tertiary"
        >已启用 {{ enabledCount }} / {{ parameters.length }}</span
      >
    </div>
    <JsonParameterImport
      v-if="importOpen"
      :parameters="parameters"
      :disabled="disabled"
      @apply="applyImportedParameters"
      @close="importOpen = false"
    />
  </section>
</template>

<style scoped>
.request-parameter-section h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 22px;
}
.request-parameter-description {
  margin: 0;
  font-size: 12px;
  line-height: 20px;
  color: var(--ant-color-text-tertiary);
}
.request-parameter-table {
  min-width: 560px;
}
.request-parameter-row {
  display: grid;
  grid-template-columns: 38px minmax(0, 1.2fr) minmax(0, 1.1fr) 110px 48px;
  align-items: start;
  gap: 10px;
}
.request-parameter-row > :first-child {
  min-height: 32px;
}
.request-parameter-row > :last-child {
  padding-top: 4px;
}
.request-parameter-header {
  align-items: center;
}
.request-parameter-header > :first-child {
  min-height: auto;
}
.request-parameter-branch {
  width: 7px;
  height: 10px;
  border-bottom: 1px solid var(--ant-color-border);
  border-left: 1px solid var(--ant-color-border);
}
.request-parameter-inactive {
  background: var(--ant-color-fill-quaternary);
}
</style>
