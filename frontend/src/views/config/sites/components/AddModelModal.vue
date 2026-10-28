<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { computed, onScopeDispose, ref } from 'vue';

import { useSiteDetailsStore, useSiteTasksStore } from '@/stores/site';

const props = defineProps<{ createModel: (siteId: string, name: string) => Promise<boolean> }>();
const emit = defineEmits<{ close: [] }>();
const { selectedSiteDetail: selected } = storeToRefs(useSiteDetailsStore());
const siteId = selected.value!.id;
const name = ref('');
const taskStore = useSiteTasksStore();
const saving = ref(false);
const disabled = computed(() => saving.value || Boolean(taskStore.tasks[siteId]));
let disposed = false;
onScopeDispose(() => {
  disposed = true;
});
async function submit() {
  if (disposed || disabled.value || !name.value.trim()) return;
  saving.value = true;
  try {
    const saved = await props.createModel(siteId, name.value);
    if (saved && !disposed) emit('close');
  } finally {
    saving.value = false;
  }
}
</script>
<template>
  <a-modal
    open
    title="添加模型"
    :width="480"
    centered
    ok-text="添加"
    cancel-text="取消"
    :ok-button-props="{ disabled: disabled || !name.trim() }"
    :confirm-loading="saving"
    :closable="!saving"
    :keyboard="!saving"
    :mask="{ closable: !saving }"
    :cancel-button-props="{ disabled: saving }"
    @cancel="!saving && emit('close')"
    @ok="submit"
  >
    <p class="m-0 leading-5.5 text-rp-secondary">
      添加到 {{ selected?.name }}；新模型默认继承站点探针策略。
    </p>
    <a-form :disabled="disabled" layout="vertical" :required-mark="false" class="pt-2">
      <a-form-item label="模型名称" class="mb-0!">
        <a-input
          :value="name"
          aria-label="模型名称"
          placeholder="输入完整模型标识"
          @update:value="!disabled && (name = $event)"
          @press-enter="submit"
        />
      </a-form-item>
    </a-form>
  </a-modal>
</template>
