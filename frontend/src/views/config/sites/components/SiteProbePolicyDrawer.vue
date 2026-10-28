<script setup lang="ts">
import type { ProbePolicy } from '@/types/domain/sites';

import { computed, onScopeDispose, ref } from 'vue';

import { savePolicy } from '@/api/config/sites';
import { useSiteDetailsStore, useSiteTasksStore } from '@/stores/site';

import ProbePolicyFields from './ProbePolicyFields.vue';

const emit = defineEmits<{ close: [] }>();
const detailsStore = useSiteDetailsStore();
const taskStore = useSiteTasksStore();
const site = detailsStore.selectedSiteDetail!;
const policy = ref<ProbePolicy>({
  ...site.policy,
  parameters: site.policy.parameters.map((parameter) => ({ ...parameter })),
});
const policyFields = ref<InstanceType<typeof ProbePolicyFields>>();
const saving = ref(false);
const disabled = computed(() => saving.value || Boolean(taskStore.tasks[site.id]));
let disposed = false;
onScopeDispose(() => {
  disposed = true;
});

async function save() {
  if (
    disposed ||
    !policyFields.value ||
    !policyFields.value.canSubmit ||
    disabled.value ||
    !taskStore.register(site.id, { type: 'save-policy' })
  )
    return;
  saving.value = true;
  try {
    await policyFields.value.validate();
    if (disposed) return;
    const input = JSON.parse(JSON.stringify(policy.value));
    const saved = await savePolicy(site.id, null, input, null);
    detailsStore.setSiteDetail(saved);
    if (!disposed) emit('close');
  } catch {
    // 表单展示校验错误，API 提示请求错误；失败时保留本次打开的草稿。
  } finally {
    taskStore.remove(site.id);
    saving.value = false;
  }
}
</script>
<template>
  <a-drawer
    open
    size="large"
    :mask-closable="false"
    :closable="{ disabled: saving }"
    :keyboard="!saving"
    @close="!saving && emit('close')"
  >
    <template #title>
      <div>
        <strong>站点默认探针策略</strong>
        <div class="mt-0.5 text-xs font-normal text-rp-tertiary">{{ site.name }}</div>
      </div>
    </template>
    <template #extra>
      <a-space>
        <a-button :disabled="saving" @click="!saving && emit('close')">取消</a-button>
        <a-button
          type="primary"
          :loading="saving"
          :disabled="disabled || !policyFields?.canSubmit"
          :title="policyFields?.submissionHint"
          @click="save"
          >保存</a-button
        >
      </a-space>
    </template>
    <ProbePolicyFields
      ref="policyFields"
      v-model="policy"
      :protocol="site.protocol"
      :disabled="disabled"
    />
  </a-drawer>
</template>
