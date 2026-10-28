<script setup lang="ts">
import type { ProbeRecordListItem, ProbeResult, RecordsPage } from '@/types/domain/records';
import type { TableEmits, TableProps } from 'antdv-next';

import { Tooltip } from 'antdv-next';
import { computed, h } from 'vue';

import {
  formatCount,
  formatDuration,
  formatRecordTime,
  resultColors,
  resultLabels,
  tablePagination,
} from '../records-presentation';

import RecordsEmptyState from './RecordsEmptyState.vue';
import AppIcon from '@/components/AppIcon.vue';

const props = defineProps<{
  data?: RecordsPage<ProbeRecordListItem>;
  loading: boolean;
  busy: boolean;
  page: number;
  pageSize: number;
}>();
const emit = defineEmits<{
  paginate: [page: number, pageSize: number];
  detail: [record: ProbeRecordListItem];
  remove: [record: ProbeRecordListItem];
  reset: [];
}>();
const columns: TableProps<ProbeRecordListItem>['columns'] = [
  {
    key: 'result',
    width: 88,
    title: () =>
      h(Tooltip, { title: '本次请求的结果，与站点或模型的当前健康状态分开记录。' }, () =>
        h('span', '状态'),
      ),
  },
  { key: 'probedAt', title: '探测时间', width: 154 },
  { key: 'siteName', dataIndex: 'siteName', title: '站点', width: 158, ellipsis: true },
  { key: 'modelName', dataIndex: 'modelName', title: '模型', width: 168, ellipsis: true },
  {
    key: 'firstTokenMs',
    title: () =>
      h('span', { class: 'inline-flex items-center gap-1' }, [
        '首 Token',
        h(
          Tooltip,
          {
            title: '从请求发出到收到首段非空思考文本或回答正文的耗时；没有数据时显示 —。',
            trigger: ['hover', 'focus'],
          },
          () =>
            h(
              'button',
              {
                type: 'button',
                'aria-label': '首 Token 耗时说明',
                class: 'cursor-help border-0 bg-transparent p-0 text-rp-tertiary',
              },
              [h(AppIcon, { name: 'QuestionCircleOutlined' })],
            ),
        ),
      ]),
    width: 128,
  },
  { key: 'durationMs', title: '总耗时', width: 112 },
  { key: 'totalTokens', title: 'Token', width: 100, align: 'right' },
  { key: 'resultMessage', title: '结果', dataIndex: 'resultMessage', ellipsis: true },
  { key: 'actions', title: '操作', width: 96, align: 'center', fixed: 'end' },
];
const pagination = computed(() =>
  tablePagination(
    props.data?.page ?? props.page,
    props.pageSize,
    props.data?.total ?? 0,
    props.loading || props.busy,
  ),
);
const change: TableEmits<ProbeRecordListItem>['change'] = (
  pagination,
  _filters,
  _sorter,
  extra,
) => {
  if (props.busy) return;
  if (extra.action === 'paginate')
    emit('paginate', pagination.current ?? 1, pagination.pageSize ?? 10);
};
</script>
<template>
  <section aria-label="探测记录列表" :aria-busy="loading">
    <a-table
      :columns="columns"
      :data-source="data?.items ?? []"
      row-key="id"
      :pagination="pagination"
      :loading="loading"
      size="medium"
      table-layout="fixed"
      :scroll="{ x: 1220 }"
      @change="change"
    >
      <template #bodyCell="{ column, record }">
        <span
          v-if="column.key === 'result'"
          class="inline-flex items-center gap-2 whitespace-nowrap"
        >
          <span
            class="size-1.5 rounded-full"
            :style="{ background: resultColors[record.result as ProbeResult] }"
          />
          {{ resultLabels[record.result as ProbeResult] }}
        </span>
        <span
          v-else-if="column.key === 'probedAt'"
          class="whitespace-nowrap text-rp-secondary tabular-nums"
          :title="formatRecordTime(record.probedAt, true)"
        >
          {{ formatRecordTime(record.probedAt) }}
        </span>
        <span v-else-if="column.key === 'firstTokenMs'" class="whitespace-nowrap tabular-nums">{{
          formatDuration(record.firstTokenMs)
        }}</span>
        <span v-else-if="column.key === 'durationMs'" class="whitespace-nowrap tabular-nums">{{
          formatDuration(record.durationMs)
        }}</span>
        <span v-else-if="column.key === 'totalTokens'" class="tabular-nums">{{
          formatCount(record.totalTokens)
        }}</span>
        <span
          v-else-if="column.key === 'resultMessage'"
          class="block truncate"
          :class="{ 'text-(--ant-color-error)': record.result === 'failure' }"
          :title="record.resultMessage"
          >{{ record.resultMessage }}</span
        >
        <div v-else-if="column.key === 'actions'" class="flex items-center justify-center gap-1">
          <a-tooltip title="查看详情">
            <a-button
              type="text"
              shape="circle"
              class="record-detail-action size-8!"
              :disabled="busy || loading"
              :aria-label="`查看探测记录 ${record.id} 的详情`"
              @click="emit('detail', record)"
            >
              <template #icon><AppIcon name="FileSearchOutlined" /></template>
            </a-button>
          </a-tooltip>
          <a-tooltip title="删除这条记录">
            <a-button
              type="text"
              shape="circle"
              danger
              class="size-8!"
              :disabled="busy || loading"
              :aria-label="`删除探测记录 ${record.id}`"
              @click="emit('remove', record)"
            >
              <template #icon><AppIcon name="DeleteOutlined" /></template>
            </a-button>
          </a-tooltip>
        </div>
      </template>
      <template #emptyText>
        <RecordsEmptyState
          kind="records"
          :loading="loading"
          :retained-count="data?.retainedCount ?? 0"
          @reset="emit('reset')"
        />
      </template>
    </a-table>
  </section>
</template>
<style scoped>
.record-detail-action:not(:disabled) {
  color: var(--ant-color-primary);
}
</style>
