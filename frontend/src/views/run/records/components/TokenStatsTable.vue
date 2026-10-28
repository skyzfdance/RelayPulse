<script setup lang="ts">
import type { SiteTokenStats, TokenStatsPage } from '@/types/domain/records';
import type { TableEmits, TableProps } from 'antdv-next';

import { computed } from 'vue';

import { formatCount, tablePagination } from '../records-presentation';

import RecordsEmptyState from './RecordsEmptyState.vue';

const props = defineProps<{
  data?: TokenStatsPage;
  loading: boolean;
  busy: boolean;
  page: number;
  pageSize: number;
}>();
const emit = defineEmits<{
  paginate: [page: number, pageSize: number];
  reset: [];
}>();
const summary = computed(() => (props.loading ? undefined : props.data?.summary));
const columns: TableProps<SiteTokenStats>['columns'] = [
  {
    key: 'siteName',
    title: '站点',
    dataIndex: 'siteName',
    ellipsis: true,
    width: 240,
    fixed: 'start',
  },
  { key: 'probeCount', title: '探测次数', dataIndex: 'probeCount', width: 118, align: 'right' },
  { key: 'reportedCount', title: '已上报', width: 118, align: 'right' },
  {
    key: 'inputTokens',
    title: '输入 Token',
    width: 150,
    align: 'right',
  },
  {
    key: 'outputTokens',
    title: '输出 Token',
    width: 150,
    align: 'right',
  },
  { key: 'totalTokens', title: '总 Token', width: 150, align: 'right' },
  { key: 'averageTokens', title: '平均每次', width: 136, align: 'right' },
];
const pagination = computed(() => {
  if ((props.data?.total ?? 0) <= 10) return false;
  return tablePagination(
    props.data?.page ?? props.page,
    props.pageSize,
    props.data?.total ?? 0,
    props.loading || props.busy,
  );
});
const change: TableEmits<SiteTokenStats>['change'] = (pagination, _filters, _sorter, extra) => {
  if (props.busy) return;
  if (extra.action === 'paginate')
    emit('paginate', pagination.current ?? 1, pagination.pageSize ?? 10);
};
</script>
<template>
  <section aria-label="Token 统计列表" :aria-busy="loading">
    <a-table
      :columns="columns"
      :data-source="data?.items ?? []"
      row-key="siteId"
      :pagination="pagination"
      :loading="loading"
      size="medium"
      table-layout="fixed"
      :scroll="{ x: 1080 }"
      @change="change"
    >
      <template #bodyCell="{ column, record }">
        <strong v-if="column.key === 'siteName'" class="block truncate" :title="record.siteName">{{
          record.siteName
        }}</strong>
        <span v-else-if="column.key === 'probeCount'" class="tabular-nums">{{
          formatCount(record.probeCount)
        }}</span>
        <span v-else-if="column.key === 'reportedCount'" class="tabular-nums"
          >{{ formatCount(record.reportedCount) }} / {{ formatCount(record.probeCount) }}</span
        >
        <strong v-else-if="column.key === 'totalTokens'" class="tabular-nums">{{
          formatCount(record.totalTokens)
        }}</strong>
        <span v-else-if="column.key === 'inputTokens'" class="tabular-nums">{{
          formatCount(record.inputTokens)
        }}</span>
        <span v-else-if="column.key === 'outputTokens'" class="tabular-nums">{{
          formatCount(record.outputTokens)
        }}</span>
        <span
          v-else-if="column.key === 'averageTokens'"
          class="tabular-nums"
          :title="`按 ${record.totalReportedCount} 次已上报总 Token 的请求计算`"
          >{{ formatCount(record.averageTokens) }}</span
        >
      </template>
      <template #summary>
        <a-table-summary v-if="summary && (data?.total ?? 0) > 0">
          <a-table-summary-row
            class="font-semibold tabular-nums"
            aria-label="当前筛选的 Token 用量合计"
          >
            <a-table-summary-cell :index="0">
              <span title="当前筛选范围内全部探测记录的合计，包含全部分页">合计</span>
            </a-table-summary-cell>
            <a-table-summary-cell :index="1" align="right">{{
              formatCount(summary.probeCount)
            }}</a-table-summary-cell>
            <a-table-summary-cell :index="2" align="right"
              >{{ formatCount(summary.reportedCount) }} /
              {{ formatCount(summary.probeCount) }}</a-table-summary-cell
            >
            <a-table-summary-cell :index="3" align="right">{{
              formatCount(summary.inputTokens)
            }}</a-table-summary-cell>
            <a-table-summary-cell :index="4" align="right">{{
              formatCount(summary.outputTokens)
            }}</a-table-summary-cell>
            <a-table-summary-cell :index="5" align="right">{{
              formatCount(summary.totalTokens)
            }}</a-table-summary-cell>
            <a-table-summary-cell :index="6" align="right">
              <span :title="`按 ${summary.totalReportedCount} 次已上报总 Token 的请求计算`">{{
                formatCount(summary.averageTokens)
              }}</span>
            </a-table-summary-cell>
          </a-table-summary-row>
        </a-table-summary>
      </template>
      <template #emptyText>
        <RecordsEmptyState
          kind="tokens"
          :loading="loading"
          :retained-count="data?.retainedCount ?? 0"
          @reset="emit('reset')"
        />
      </template>
    </a-table>
  </section>
</template>
