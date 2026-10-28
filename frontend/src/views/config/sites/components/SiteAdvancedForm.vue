<script setup lang="ts">
import type { ConnectionNumberField } from '../connection-validation';
import type { Connection, RequestHeader } from '@/types/domain/sites';
import type { FormInstance } from 'antdv-next';

import { nextTick, ref } from 'vue';

import {
  connectionNumberError,
  connectionNumberFields,
  endpointError,
  headerNameError,
  headerValueError,
} from '../connection-validation';

import AppIcon from '@/components/AppIcon.vue';

const connection = defineModel<Connection>({ required: true });
const props = defineProps<{ baseUrl: string; disabled?: boolean }>();
const form = ref<FormInstance>();
const numberInputs: Partial<Record<ConnectionNumberField, string>> = {};

function validateResult(error: string | null) {
  return error ? Promise.reject(new Error(error)) : Promise.resolve();
}
async function onNumberInput(name: ConnectionNumberField, text: string) {
  if (props.disabled) return;
  numberInputs[name] = text;
  await nextTick();
  // 无法解析的输入不会触发数字组件的 change，仍须显示字段校验结果。
  await form.value?.validateFields([name]).catch(() => {});
}
function update<K extends keyof Connection>(field: K, value: Connection[K]) {
  if (!props.disabled) connection.value[field] = value;
}
function updateHeader(index: number, field: keyof RequestHeader, value: string) {
  if (!props.disabled) connection.value.headers[index][field] = value;
}
function addHeader() {
  if (!props.disabled) connection.value.headers.push({ key: '', value: '' });
}
function removeHeader(index: number) {
  if (!props.disabled) connection.value.headers.splice(index, 1);
}
function validate() {
  return form.value!.validateFields();
}
defineExpose({ validate });
</script>
<template>
  <a-form
    ref="form"
    :model="connection"
    layout="vertical"
    :required-mark="false"
    :disabled="disabled"
    :validate-trigger="['change', 'blur']"
  >
    <div class="grid grid-cols-2 gap-x-5">
      <a-form-item
        label="模型列表端点"
        name="modelsPath"
        :rules="[
          {
            validator: (_rule: unknown, value: unknown) =>
              validateResult(endpointError(value, '模型列表端点', props.baseUrl)),
          },
        ]"
      >
        <a-input :value="connection.modelsPath" @update:value="update('modelsPath', $event)" />
      </a-form-item>
      <a-form-item
        label="探测端点"
        name="probePath"
        :rules="[
          {
            validator: (_rule: unknown, value: unknown) =>
              validateResult(endpointError(value, '探测端点', props.baseUrl)),
          },
        ]"
      >
        <a-input :value="connection.probePath" @update:value="update('probePath', $event)" />
      </a-form-item>
    </div>
    <div class="mb-3 overflow-hidden rounded-md border border-rp-line">
      <div
        class="grid min-h-10 grid-cols-[1fr_1.5fr_40px] items-center border-b border-rp-line bg-rp-subtle px-3 text-xs text-rp-secondary"
      >
        <span>请求头</span><span>值</span><span />
      </div>
      <div
        v-for="(header, index) in connection.headers"
        :key="index"
        class="grid min-h-10 grid-cols-[1fr_1.5fr_40px] items-start border-b border-rp-line"
      >
        <a-form-item
          :name="['headers', index, 'key']"
          class="mb-0! min-w-0"
          :rules="[
            {
              validator: (_rule: unknown, value: unknown) =>
                validateResult(headerNameError(value, connection.headers, index)),
            },
          ]"
        >
          <a-input
            :value="header.key"
            variant="borderless"
            :aria-label="`请求头名称 ${index + 1}`"
            placeholder="Header"
            @update:value="updateHeader(index, 'key', $event)"
          />
        </a-form-item>
        <a-form-item
          :name="['headers', index, 'value']"
          class="mb-0! min-w-0"
          :rules="[
            {
              validator: (_rule: unknown, value: unknown) =>
                validateResult(headerValueError(value)),
            },
          ]"
        >
          <a-input
            :value="header.value"
            variant="borderless"
            :aria-label="`请求头值 ${index + 1}`"
            placeholder="Value"
            @update:value="updateHeader(index, 'value', $event)"
          />
        </a-form-item>
        <a-button
          type="text"
          danger
          :aria-label="`删除请求头 ${index + 1}`"
          @click="removeHeader(index)"
          ><template #icon><AppIcon name="DeleteOutlined" /></template
        ></a-button>
      </div>
      <a-button type="link" class="h-10!" @click="addHeader"
        ><template #icon><AppIcon name="PlusOutlined" /></template>添加请求头</a-button
      >
    </div>
    <div class="divide-y divide-rp-line border-t border-rp-line">
      <a-form-item
        v-for="(field, name) in connectionNumberFields"
        :key="name"
        :name="name"
        :label="field.label"
        :tooltip="field.tooltip"
        :rules="[
          {
            validator: (_rule: unknown, value: unknown) =>
              validateResult(connectionNumberError(name, value, numberInputs[name])),
          },
        ]"
        layout="horizontal"
        label-align="left"
        :label-col="{ flex: '1' }"
        :wrapper-col="{ flex: '1' }"
        :colon="false"
        class="probe-setting-row mb-0! py-3!"
      >
        <a-input-number
          :value="connection[name]"
          :suffix="field.unit"
          :min="1"
          :max="field.max"
          :change-on-blur="false"
          :formatter="(value: unknown) => numberInputs[name] ?? String(value ?? '')"
          class="w-28! shrink-0"
          @update:value="update(name, $event)"
          @input="onNumberInput(name, $event)"
          @step="onNumberInput(name, String($event))"
        />
      </a-form-item>
    </div>
  </a-form>
</template>
<style scoped>
.probe-setting-row :deep(.ant-form-item-control-input-content) {
  display: flex;
  justify-content: flex-end;
}
.probe-setting-row :deep(.ant-form-item-explain-error) {
  text-align: right;
}
</style>
