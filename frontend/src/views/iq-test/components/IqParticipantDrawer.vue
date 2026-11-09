<script setup lang="ts">
import type { RequestParameterDraft } from '@/types/request-parameters';

import { computed, ref, watch } from 'vue';

import { useIqTestStore } from '@/stores/iq-test';
import { parameterEnabled } from '@/utils/request-parameters';

import { buildIqParameters, iqParameterError, iqPresets } from '../iq-parameters';

import RequestParameterEditor from '@/components/request-parameter/editor.vue';
import RequestParameterPreview from '@/components/request-parameter/preview.vue';

const open = defineModel<boolean>('open', { default: false });
const store = useIqTestStore();
const modelKey = ref<string>();
const preset = ref('high');
const parameters = ref<RequestParameterDraft[]>([]);
const saving = ref(false);
const candidate = computed(() => store.candidates.find((item) => item.modelKey === modelKey.value));
const presets = computed(() => iqPresets(candidate.value?.protocol ?? 'openai-compatible'));
const groups = computed(() => {
  const sites = new Map<string, { label: string; options: { label: string; value: string }[] }>();
  for (const item of store.candidates) {
    if (!sites.has(item.siteId)) sites.set(item.siteId, { label: item.siteName, options: [] });
    sites.get(item.siteId)!.options.push({ label: item.modelName, value: item.modelKey });
  }
  return [...sites.values()];
});
const error = computed(() => iqParameterError(parameters.value));
const preview = computed(() => {
  if (!candidate.value) return '请先选择模型后查看预览。';
  if (error.value) return '请先修正配置后查看预览。';
  return JSON.stringify(
    buildIqParameters(candidate.value.protocol, preset.value, parameters.value),
    null,
    2,
  );
});
watch(open, (value) => {
  if (!value) return;
  modelKey.value = undefined;
  parameters.value = [];
  preset.value = 'high';
  void store.refresh();
});
watch([() => candidate.value?.modelKey, () => candidate.value?.protocol], () => {
  preset.value = candidate.value?.protocol === 'custom-http' ? 'custom' : 'high';
});
async function submit() {
  if (!candidate.value || error.value || saving.value) return;
  saving.value = true;
  try {
    const count = parameters.value.filter((p) => parameterEnabled(parameters.value, p)).length;
    await store.addParticipant({
      modelKey: candidate.value.modelKey,
      preset:
        preset.value === 'custom'
          ? `自定义 · ${count} 项参数`
          : presets.value.find((p) => p.value === preset.value)!.label,
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
      <a-form-item label="推理强度">
        <a-select
          v-model:value="preset"
          aria-label="推理强度"
          :options="presets"
          :disabled="!candidate"
          class="w-full"
        />
      </a-form-item>
    </a-form>
    <RequestParameterEditor
      v-model:parameters="parameters"
      :disabled="saving"
      :error="error"
      description="手写注入字段，优先级高于预设档位"
      hint="支持对象路径；关闭父级时子参数不发送。题面、model、stream 为系统保留字段。"
      empty-text="暂无手写参数"
    />
    <RequestParameterPreview
      :value="preview"
      title="IQ 参数预览"
      description="合并推理强度和已启用的手写参数"
      class="mt-5"
    />
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
