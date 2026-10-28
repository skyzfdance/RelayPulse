<script setup lang="ts">
import type { SyncConfirmation } from '../use-site-detail';

import { computed } from 'vue';

import { useSiteTasksStore } from '@/stores/site';

const props = defineProps<{ confirmation?: SyncConfirmation }>();
const emit = defineEmits<{
  close: [];
  confirm: [];
}>();
const taskStore = useSiteTasksStore();
const busy = computed(() =>
  Boolean(props.confirmation && taskStore.tasks[props.confirmation.siteId]),
);
const syncing = computed(() =>
  Boolean(props.confirmation && taskStore.tasks[props.confirmation.siteId]?.type === 'sync'),
);
</script>
<template>
  <a-modal
    v-if="confirmation"
    open
    :title="`${confirmation.siteName} · 确认同步模型目录`"
    ok-text="确认同步"
    cancel-text="取消同步"
    :confirm-loading="syncing"
    :ok-button-props="{ disabled: busy }"
    :keyboard="!syncing"
    :closable="!syncing"
    :mask="{ closable: !syncing }"
    :cancel-button-props="{ disabled: syncing }"
    @cancel="!syncing && emit('close')"
    @ok="!busy && emit('confirm')"
  >
    <section v-if="confirmation.added.length" aria-label="新增模型">
      <p class="font-medium">将新增 {{ confirmation.added.length }} 个模型</p>
      <ul class="mt-2">
        <li v-for="name in confirmation.added" :key="name">{{ name }}</li>
      </ul>
    </section>
    <section v-if="confirmation.removed.length" class="mt-4" aria-label="删除模型">
      <a-alert
        title="以下模型已不在远端目录中，确认后将删除模型及其独立设置。"
        type="warning"
        show-icon
      />
      <ul class="mt-2">
        <li v-for="model in confirmation.removed" :key="model.id">{{ model.name }}</li>
      </ul>
    </section>
  </a-modal>
</template>
