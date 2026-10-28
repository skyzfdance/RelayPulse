<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { computed } from 'vue';

import { useSiteDetailsStore, useSiteTasksStore } from '@/stores/site';

import { healthLabels } from '../site-policy';

import AppIcon from '@/components/AppIcon.vue';

const props = defineProps<{ dirty: boolean }>();
const emit = defineEmits<{
  add: [];
  settings: [modelId: string];
  sync: [siteId: string];
  probe: [siteId: string, modelId?: string];
  toggle: [siteId: string, modelId: string, enabled: boolean];
  remove: [siteId: string, modelId: string];
}>();
const { selectedSiteDetail: site } = storeToRefs(useSiteDetailsStore());
const taskStore = useSiteTasksStore();
const task = computed(() => site.value && taskStore.tasks[site.value.id]);
const disabled = computed(() => props.dirty || Boolean(task.value));
const probingModelId = computed(() =>
  task.value?.type === 'probe-model' ? task.value.modelId : undefined,
);
const columns = [
  { key: 'enabled', title: '启用', width: 72, align: 'center' as const, fixed: 'left' as const },
  { key: 'name', dataIndex: 'name', title: '模型', width: 230, ellipsis: true },
  { key: 'policy', title: '推理设置', width: 190, ellipsis: true },
  { key: 'firstToken', title: '首 Token', width: 125 },
  { key: 'duration', title: '总耗时', width: 110 },
  { key: 'actions', title: '操作', width: 140, align: 'center' as const, fixed: 'right' as const },
];
</script>
<template>
  <div v-if="site">
    <div class="mb-2.5 flex flex-wrap items-center justify-between gap-4">
      <div>
        <h3 class="m-0! text-sm! leading-5.5! font-semibold!">模型探针</h3>
        <p class="m-0! text-xs leading-5 text-rp-tertiary">
          {{ site.models.filter((model) => model.enabled).length }} /
          {{ site.models.length }} 个模型已启用
        </p>
      </div>
      <div class="flex gap-2">
        <a-button
          :disabled="disabled || !site.models.some((m) => m.enabled)"
          :loading="task?.type === 'probe'"
          @click="!disabled && site.models.some((model) => model.enabled) && emit('probe', site.id)"
        >
          <template #icon><AppIcon name="ExperimentOutlined" /></template>探测已启用模型
        </a-button>
        <a-button
          :disabled="disabled"
          :loading="task?.type === 'sync'"
          @click="!disabled && emit('sync', site.id)"
        >
          <template #icon><AppIcon name="SyncOutlined" /></template>同步模型
        </a-button>
        <a-button type="primary" :disabled="disabled" @click="!disabled && emit('add')">
          <template #icon><AppIcon name="PlusOutlined" /></template>添加模型
        </a-button>
      </div>
    </div>
    <a-table
      :columns="columns"
      :data-source="site.models"
      row-key="id"
      :pagination="false"
      size="small"
      table-layout="fixed"
      :scroll="{ x: 900 }"
    >
      <template #headerCell="{ column }">
        <span v-if="column.key === 'firstToken'" class="inline-flex items-center gap-1">
          首 Token
          <a-tooltip title="从请求发出到收到首段非空思考文本或回答正文的耗时；未取得数据时显示 —。">
            <AppIcon name="QuestionCircleOutlined" aria-label="首 Token 数据说明" />
          </a-tooltip>
        </span>
      </template>
      <template #bodyCell="{ column, record }">
        <a-switch
          v-if="column.key === 'enabled'"
          :checked="record.enabled"
          size="small"
          :disabled="disabled"
          :aria-label="`${record.enabled ? '停用' : '启用'}模型 ${record.name}`"
          @change="!disabled && emit('toggle', site.id, record.id, $event as boolean)"
        />
        <span
          v-else-if="column.key === 'name'"
          class="block truncate font-semibold"
          :title="`${record.name} · ${healthLabels[record.health as keyof typeof healthLabels]}`"
          >{{ record.name }}</span
        >
        <span v-else-if="column.key === 'policy'" class="text-rp-secondary">{{
          record.customRequest ? '自定义请求' : record.policy ? '独立策略' : '继承站点默认'
        }}</span>
        <span v-else-if="column.key === 'firstToken'">{{
          record.customRequest || record.firstTokenMs === null
            ? '—'
            : `${(record.firstTokenMs / 1000).toFixed(2)} s`
        }}</span>
        <span v-else-if="column.key === 'duration'">{{
          record.durationMs === null ? '—' : `${(record.durationMs / 1000).toFixed(2)} s`
        }}</span>
        <div v-else-if="column.key === 'actions'" class="flex items-center justify-center gap-1">
          <a-tooltip title="立即探测模型">
            <a-button
              type="text"
              shape="circle"
              class="model-action size-8!"
              :disabled="disabled"
              :loading="probingModelId === record.id"
              :aria-label="`立即探测模型 ${record.name}`"
              @click="!disabled && emit('probe', site.id, record.id)"
            >
              <template #icon><AppIcon name="PlayCircleOutlined" /></template>
            </a-button>
          </a-tooltip>
          <a-tooltip title="模型探针设置">
            <a-button
              type="text"
              shape="circle"
              class="model-action size-8!"
              :disabled="disabled"
              :aria-label="`设置模型 ${record.name}`"
              @click="!disabled && emit('settings', record.id)"
            >
              <template #icon><AppIcon name="SettingOutlined" /></template>
            </a-button>
          </a-tooltip>
          <a-popconfirm
            :title="`删除模型 ${record.name}？`"
            description="模型的独立设置也会一并删除。"
            ok-text="删除"
            cancel-text="取消"
            :ok-button-props="{ danger: true }"
            :disabled="disabled"
            @confirm="!disabled && emit('remove', site.id, record.id)"
          >
            <a-button
              type="text"
              shape="circle"
              danger
              class="size-8!"
              :disabled="disabled"
              :aria-label="`删除模型 ${record.name}`"
            >
              <template #icon><AppIcon name="DeleteOutlined" /></template>
            </a-button>
          </a-popconfirm>
        </div>
      </template>
    </a-table>
  </div>
</template>
<style scoped>
.model-action:not(:disabled) {
  color: var(--ant-color-primary);
}
</style>
