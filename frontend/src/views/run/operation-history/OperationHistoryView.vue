<script setup lang="ts">
import type {
  OperationActionGroup,
  OperationLogListItem,
  OperationLogQuery,
  OperationResult,
  OperationSource,
} from '@/types/domain/operation-logs';
import type { TableProps } from 'antdv-next';

import dayjs from 'dayjs';
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { operationActionGroupLabels } from '@/types/domain/operation-logs';

import { saveOperationLogView, takeOperationLogView } from './operation-log-navigation';
import { parseOperationLogQuery, serializeOperationLogQuery } from './operation-log-query';
import { useOperationLogs } from './use-operation-logs';

import OperationLogDetailDrawer from './components/OperationLogDetailDrawer.vue';
import AppIcon from '@/components/AppIcon.vue';

const route = useRoute();
const router = useRouter();
const query = reactive(parseOperationLogQuery(route.query));
const activeLogId = ref('');
const tableViewport = ref<HTMLElement>();
const logState = useOperationLogs();
const { data, loading, error, detail, detailLoading, detailError, detailMissing } = logState;
const sourceOptions = [
  { label: '系统', value: 'system' },
  { label: '用户', value: 'user' },
];
const resultOptions = [
  { label: '成功', value: 'success' },
  { label: '失败', value: 'failure' },
  { label: '已取消', value: 'cancelled' },
  { label: '受阻', value: 'blocked' },
];
const actionOptions = computed(() =>
  [
    ...new Set([...Object.keys(operationActionGroupLabels), ...(data.value?.actionGroups ?? [])]),
  ].map((value) => ({ label: operationActionGroupLabels[value] ?? value, value })),
);
const datePresets = [
  { days: 7, label: '最近一周' },
  { days: 15, label: '最近15天' },
  { days: 30, label: '最近30天' },
].map(({ days, label }) => ({
  label,
  value: [
    () =>
      dayjs()
        .subtract(days - 1, 'day')
        .startOf('day'),
    () => dayjs().endOf('day'),
  ],
}));
const columns: TableProps<OperationLogListItem>['columns'] = [
  { key: 'source', dataIndex: 'source', title: '来源', width: 82 },
  { key: 'actionGroup', dataIndex: 'actionGroup', title: '动作', width: 100 },
  { key: 'summary', dataIndex: 'summary', title: '操作概要', width: 520 },
  { key: 'result', dataIndex: 'result', title: '结果', width: 100 },
  { key: 'occurredAt', dataIndex: 'occurredAt', title: '时间', width: 176 },
  { key: 'operation', title: '操作', width: 72, fixed: 'end', align: 'center' },
];
const busy = computed(() => loading.value);
const showTotal = (total: number, range: [number, number]) =>
  `第 ${range[0]}-${range[1]} 条，共 ${total} 条`;
const formatTime = (value: string) =>
  new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  }).format(new Date(value));

async function load() {
  const next = await logState.load({ ...query });
  if (next && next.page !== query.page) {
    query.page = next.page;
    await updateRoute();
  }
}

async function updateRoute() {
  await router.replace({
    query: {
      ...route.query,
      ...serializeOperationLogQuery(query),
      startDate: query.startDate || undefined,
      endDate: query.endDate || undefined,
      source: query.source.length ? query.source.join(',') : undefined,
      result: query.result.length ? query.result.join(',') : undefined,
      actions: query.actions.length ? query.actions.join(',') : undefined,
      keyword: undefined,
    },
  });
}

async function updateFilter(patch: Partial<OperationLogQuery>) {
  Object.assign(query, patch, { page: 1 });
  await updateRoute();
}

function changeDateRange(_dates: unknown, dateStrings?: unknown) {
  let values: unknown[] = [];
  if (Array.isArray(dateStrings)) values = dateStrings;
  else if (Array.isArray(_dates)) values = _dates;
  const startDate = typeof values[0] === 'string' ? values[0] : null;
  const endDate = typeof values[1] === 'string' ? values[1] : null;
  void updateFilter({ startDate, endDate });
}

function paginate(page: number, pageSize: number) {
  query.page = pageSize === query.pageSize ? page : 1;
  query.pageSize = pageSize;
  void updateRoute();
}

function showDetail(logId: string) {
  activeLogId.value = logId;
  void logState.loadDetail(logId);
}

function closeDetail() {
  activeLogId.value = '';
  logState.closeDetail();
}

function openProbeRecords(item: OperationLogListItem) {
  const behavior = item.detailBehavior;
  if (
    behavior.kind !== 'probe_records' ||
    behavior.availability === 'none' ||
    !behavior.recordIds.length
  ) {
    showDetail(item.id);
    return;
  }
  void router.push({
    name: 'records',
    query: {
      recordIds: behavior.recordIds.join(','),
      fromOperationHistory: '1',
    },
  });
}

function viewDetail(item: OperationLogListItem) {
  if (loading.value || error.value) return;
  if (item.detailBehavior.kind === 'probe_records' && item.detailBehavior.availability !== 'none') {
    if (data.value)
      saveOperationLogView(route.fullPath, {
        page: data.value,
        scrollTop: tableViewport.value?.scrollTop ?? 0,
      });
    openProbeRecords(item);
  } else showDetail(item.id);
}

watch(
  () => route.fullPath,
  () => {
    Object.assign(query, parseOperationLogQuery(route.query));
    if (tableViewport.value) tableViewport.value.scrollTop = 0;
    void load();
  },
);
onMounted(async () => {
  const saved = takeOperationLogView(route.fullPath);
  if (!saved) {
    await load();
    return;
  }
  data.value = saved.page;
  await nextTick();
  if (tableViewport.value) tableViewport.value.scrollTop = saved.scrollTop;
});

const resultLabel: Record<OperationResult, string> = {
  success: '成功',
  failure: '失败',
  cancelled: '已取消',
  blocked: '受阻',
};
const resultColor: Record<OperationResult, string> = {
  success: 'var(--ant-color-success)',
  failure: 'var(--ant-color-error)',
  cancelled: 'var(--ant-color-text-tertiary)',
  blocked: 'var(--ant-color-warning)',
};
const sourceLabel: Record<OperationSource, string> = { system: '系统', user: '用户' };
const labelSource = (source: OperationSource) => sourceLabel[source];
const labelAction = (action: OperationActionGroup) => operationActionGroupLabels[action] ?? action;
const labelResult = (result: OperationResult) => resultLabel[result];
const colorResult = (result: OperationResult) => resultColor[result];
const linksToProbeRecords = (item: OperationLogListItem) =>
  item.detailBehavior.kind === 'probe_records' && item.detailBehavior.availability !== 'none';
const operationTooltip = (item: OperationLogListItem) =>
  linksToProbeRecords(item) ? '查看关联探测记录' : '查看操作详情';
const activeFilters = computed(() =>
  Boolean(
    query.startDate ||
    query.endDate ||
    query.source.length ||
    query.result.length ||
    query.actions.length,
  ),
);
</script>

<template>
  <div class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-6">
    <div
      class="mb-4 flex min-w-0 flex-none flex-wrap items-center gap-2 border-b border-rp-line pb-4"
    >
      <a-range-picker
        :value="[query.startDate, query.endDate]"
        value-format="YYYY-MM-DD"
        format="YYYY-MM-DD"
        :presets="datePresets"
        :allow-empty="[true, true]"
        :placeholder="['开始日期', '结束日期']"
        class="w-full min-w-0 shrink-0 sm:w-80"
        aria-label="时间范围"
        @change="changeDateRange"
      />
      <a-select
        :value="query.source"
        mode="multiple"
        allow-clear
        max-tag-count="responsive"
        class="w-full min-w-0 shrink-0 sm:w-44"
        aria-label="操作来源"
        placeholder="全部来源"
        :options="sourceOptions"
        @change="updateFilter({ source: $event as OperationSource[] })"
      />
      <a-select
        :value="query.result"
        mode="multiple"
        allow-clear
        max-tag-count="responsive"
        class="w-full min-w-0 shrink-0 sm:w-40"
        aria-label="执行结果"
        placeholder="全部结果"
        :options="resultOptions"
        @change="updateFilter({ result: $event as OperationResult[] })"
      />
      <a-select
        :value="query.actions"
        mode="multiple"
        allow-clear
        max-tag-count="responsive"
        class="w-full min-w-0 shrink-0 sm:w-48"
        aria-label="操作动作"
        placeholder="全部动作"
        :options="actionOptions"
        @change="updateFilter({ actions: $event as OperationActionGroup[] })"
      />
    </div>

    <a-alert
      v-if="error"
      class="mb-3"
      type="error"
      show-icon
      title="操作记录加载失败"
      :description="error"
      role="alert"
    >
      <template #action><a-button :loading="loading" @click="load">重新加载</a-button></template>
    </a-alert>

    <section
      ref="tableViewport"
      class="min-h-0 flex-1 overflow-auto"
      aria-label="操作记录列表"
      :aria-busy="loading"
    >
      <a-table
        :columns="columns"
        :data-source="data?.items ?? []"
        row-key="id"
        size="small"
        :loading="loading"
        :pagination="false"
        :scroll="{ x: 1100 }"
        table-layout="fixed"
      >
        <template #bodyCell="{ column, record }">
          <time
            v-if="column.key === 'occurredAt'"
            class="whitespace-nowrap tabular-nums text-rp-secondary"
            :datetime="record.occurredAt"
            :title="formatTime(record.occurredAt)"
          >
            {{ formatTime(record.occurredAt) }}
          </time>
          <a-tag
            v-else-if="column.key === 'source'"
            :color="record.source === 'system' ? 'blue' : 'geekblue'"
            variant="outlined"
            class="m-0!"
            :title="record.actorSnapshot"
            >{{ labelSource(record.source)
            }}<span class="sr-only">，{{ record.actorSnapshot }}</span></a-tag
          >
          <span v-else-if="column.key === 'actionGroup'" class="whitespace-nowrap">{{
            labelAction(record.actionGroup)
          }}</span>
          <span v-else-if="column.key === 'summary'" v-overflow-tooltip>{{ record.summary }}</span>
          <span
            v-else-if="column.key === 'result'"
            class="inline-flex items-center gap-2 whitespace-nowrap"
          >
            <span
              class="size-1.5 rounded-full"
              :style="{ background: colorResult(record.result) }"
            />{{ labelResult(record.result) }}
          </span>
          <a-tooltip v-else-if="column.key === 'operation'" :title="operationTooltip(record)">
            <a-button
              variant="text"
              :color="linksToProbeRecords(record) ? 'cyan' : 'primary'"
              size="small"
              shape="circle"
              :aria-label="operationTooltip(record)"
              :disabled="loading || Boolean(error)"
              @click="viewDetail(record)"
            >
              <template #icon><AppIcon name="FileSearchOutlined" /></template>
            </a-button>
          </a-tooltip>
        </template>
        <template #emptyText>
          <div class="py-10 text-center">
            <a-empty
              :description="
                loading
                  ? '正在加载操作记录…'
                  : activeFilters
                    ? '没有符合条件的操作记录'
                    : '暂无操作记录'
              "
            >
            </a-empty>
          </div>
        </template>
      </a-table>
    </section>

    <div
      v-if="data && data.total > 0"
      class="flex flex-none flex-wrap items-center justify-end gap-3 pt-3"
    >
      <a-pagination
        :current="data.page"
        :page-size="data.pageSize"
        :page-size-options="['10', '20', '50', '100']"
        :show-size-changer="true"
        :show-quick-jumper="false"
        :show-total="showTotal"
        size="small"
        :total="data.total"
        :disabled="busy"
        @change="paginate"
      />
    </div>

    <OperationLogDetailDrawer
      :open="Boolean(activeLogId)"
      :loading="detailLoading"
      :error="detailError"
      :missing="detailMissing"
      :detail="detail"
      @close="closeDetail"
      @retry="activeLogId && logState.loadDetail(activeLogId)"
    />
  </div>
</template>
