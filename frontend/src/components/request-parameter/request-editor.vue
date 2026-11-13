<script setup lang="ts">
import type {
  RequestParameterDraft,
  RequestParameterValidator,
  RequestPreview,
} from '@/types/request-parameters';

import { ref } from 'vue';

import RequestParameterEditor from './editor.vue';
import RequestParameterPreview from './preview.vue';

const headers = defineModel<RequestParameterDraft[]>('headers', { required: true });
const parameters = defineModel<RequestParameterDraft[]>('parameters', { required: true });
defineProps<{
  disabled?: boolean;
  modelName?: string;
  error?: string;
  headerValidation?: RequestParameterValidator;
  bodyDescription?: string;
  bodyHint?: string;
  preview: RequestPreview | string;
  previewDescription?: string;
}>();
const active = ref('headers');
const tabs = [
  { key: 'headers', label: '请求头 Headers', forceRender: true },
  { key: 'body', label: '请求体 Body', forceRender: true },
  { key: 'preview', label: '请求预览', forceRender: true },
];
</script>

<template>
  <a-tabs v-model:active-key="active" :items="tabs">
    <template #contentRender="{ item }">
      <template v-if="item.key === 'headers'">
        <slot name="headers-before" />
        <RequestParameterEditor
          v-model:parameters="headers"
          mode="headers"
          title="请求头 Headers"
          data-request-section="headers"
          :validation="headerValidation"
          :disabled="disabled"
        />
      </template>
      <template v-else-if="item.key === 'body'">
        <slot name="body-before" />
        <RequestParameterEditor
          v-model:parameters="parameters"
          title="请求体 Body"
          data-request-section="body"
          :model-name="modelName"
          :disabled="disabled"
          :error="error"
          :description="bodyDescription"
          :hint="bodyHint"
        />
      </template>
      <RequestParameterPreview v-else :value="preview" :description="previewDescription">
        <template #extra><slot name="preview-extra" /></template>
        <slot name="preview-result" />
      </RequestParameterPreview>
    </template>
  </a-tabs>
</template>
