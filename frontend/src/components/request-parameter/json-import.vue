<script setup lang="ts">
import type { RequestParameterDraft, RequestParameterValidator } from '@/types/request-parameters';

import { ref } from 'vue';

import { mergeRequestHeadersFromJson } from '@/utils/request-headers';
import { mergeRequestParametersFromJson } from '@/utils/request-parameters';

defineOptions({ name: 'RequestParameterJsonImport' });

const props = defineProps<{
  parameters: RequestParameterDraft[];
  disabled?: boolean;
  mode?: 'body' | 'headers';
  validation?: RequestParameterValidator;
}>();
const emit = defineEmits<{ apply: [parameters: RequestParameterDraft[]]; close: [] }>();
const json = ref('');
const error = ref('');
const placeholder =
  '{\n  "temperature": 0.7,\n  "max_tokens": 1024,\n  "reasoning": { "effort": "high" }\n}';

function updateJson(value: string) {
  if (props.disabled) return;
  json.value = value;
  error.value = '';
}

function submit() {
  if (props.disabled) return;
  try {
    const imported = (
      props.mode === 'headers' ? mergeRequestHeadersFromJson : mergeRequestParametersFromJson
    )(props.parameters, json.value);
    const validationError = props
      .validation?.(imported)
      .flatMap((item) => [item.key, item.value])
      .find(Boolean);
    if (validationError) throw new Error(validationError);
    emit('apply', imported);
    emit('close');
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'JSON 参数解析失败，请检查格式';
  }
}
</script>

<template>
  <a-modal
    open
    title="粘贴 JSON 参数"
    :width="640"
    centered
    ok-text="解析并导入"
    cancel-text="取消"
    :ok-button-props="{ disabled: disabled || !json.trim() }"
    :mask="{ closable: false }"
    @cancel="emit('close')"
    @ok="submit"
  >
    <p class="mt-0! mb-3! text-xs leading-5 text-rp-secondary">
      {{
        mode === 'headers'
          ? '粘贴字符串键值的 JSON 对象；同名请求头覆盖现有值，其余请求头保留。'
          : '粘贴 JSON 对象，自动识别参数类型；同名参数覆盖现有值，其余参数保留。嵌套对象按字段合并。'
      }}
    </p>
    <a-textarea
      :value="json"
      :disabled="disabled"
      :status="error ? 'error' : undefined"
      :aria-invalid="Boolean(error)"
      aria-label="JSON 参数"
      :placeholder="
        mode === 'headers' ? '{\n  &quot;x-route&quot;: &quot;custom-value&quot;\n}' : placeholder
      "
      :auto-size="{ minRows: 10, maxRows: 18 }"
      :spellcheck="false"
      class="font-mono text-xs!"
      @update:value="updateJson"
    />
    <p v-if="error" role="alert" class="mt-2! mb-0! text-xs text-rp-error">{{ error }}</p>
    <p
      v-if="parameters.some((parameter) => parameter.binding === 'model')"
      class="mt-2! mb-0! text-xs text-rp-tertiary"
    >
      model 字段继续使用当前模型。
    </p>
  </a-modal>
</template>
