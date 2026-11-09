<script setup lang="ts">
import type { IqHistoryResult, IqHistoryResultSummary } from './iq-history';
import type { IqRun } from '@/types/domain/iq-test';
import type { TableProps } from 'antdv-next';

import dayjs from 'dayjs';
import { computed, onMounted, onScopeDispose, ref, shallowRef, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { useIqTestStore } from '@/stores/iq-test';

import { formatIqDuration, formatIqStrength, formatIqTime } from '../iq-presentation';
import { filterIqHistory, getIqHistoryResult } from './iq-history';
import { vSiteModelSelection } from './site-model-selection';
import { useIqHistoryFilterOptions } from './use-iq-history-filter-options';

import IqRunDetail from '../components/IqRunDetail.vue';
import AppIcon from '@/components/AppIcon.vue';

const store = useIqTestStore();
const route = useRoute();
const router = useRouter();
const dateRange = ref<[string | null, string | null]>([null, null]);
const participantPaths = ref<string[][]>([]);
const result = ref<IqHistoryResult>();
const {
  options,
  loading: optionsLoading,
  error: optionsError,
  load: loadOptions,
} = useIqHistoryFilterOptions();
const busy = ref(false);
const detail = shallowRef<IqRun | null>(null);
const detailLoading = ref(false);
const detailError = ref('');
const page = ref(1);
const pageSize = ref(20);
const tableViewport = ref<HTMLElement>();
let detailVersion = 0;
let explicitRefreshes = 0;
let disposed = false;
type HistoryRow = IqRun & { callResult: IqHistoryResultSummary };
const columns: TableProps<HistoryRow>['columns'] = [
  { key: 'startedAtMs', title: '测试时间', width: 178 },
  { key: 'questions', title: '测试题目', width: 260 },
  { key: 'participants', title: '参与者', width: 220 },
  { key: 'result', title: '调用结果', width: 115 },
  { key: 'durationMs', title: '总耗时', width: 115 },
  { key: 'actions', title: '操作', width: 100, align: 'center', fixed: 'end' },
];
const datePresets = [
  { days: 7, label: '最近一周' },
  { days: 30, label: '最近一个月' },
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
const resultOptions = [
  { value: 'all-success', label: '全部成功' },
  { value: 'partial-failure', label: '部分失败' },
  { value: 'all-failure', label: '全部失败' },
  { value: 'aborted', label: '已中止' },
];
const filtered = computed(() =>
  filterIqHistory(store.runs, {
    startAtMs: dateRange.value[0] ? dayjs(dateRange.value[0]).startOf('day').valueOf() : null,
    endAtMs: dateRange.value[1] ? dayjs(dateRange.value[1]).endOf('day').valueOf() : null,
    participants: participantPaths.value,
    result: result.value ?? '',
  }),
);
const hasFilters = computed(() =>
  Boolean(dateRange.value.some(Boolean) || participantPaths.value.length || result.value),
);
const detailId = computed(() => (typeof route.query.runId === 'string' ? route.query.runId : null));
const visibleRuns = computed(() => {
  const start = (page.value - 1) * pageSize.value;
  return filtered.value
    .slice(start, start + pageSize.value)
    .map((run) => ({ ...run, callResult: getIqHistoryResult(run) }));
});
const showTotal = (total: number, range: [number, number]) =>
  `第 ${range[0]}–${range[1]} 条，共 ${total} 条`;
watch([dateRange, participantPaths, result], () => {
  page.value = 1;
  resetScroll();
  void store.refresh();
});
watch(filtered, (runs) => {
  page.value = Math.min(page.value, Math.max(1, Math.ceil(runs.length / pageSize.value)));
});
watch([page, pageSize], resetScroll, { flush: 'post' });
watch(detailId, () => void loadDetail(), { immediate: true });
// 只用历史身份检测目录失效；级联树始终由独立接口返回。
watch(
  () =>
    store.runs
      .filter((run) => run.status !== 'running')
      .map((run) => run.id)
      .join('\n'),
  () => {
    if (!explicitRefreshes) void loadOptions();
  },
);

function changeDateRange(_dates: unknown, dateStrings?: unknown) {
  const values = Array.isArray(dateStrings) ? dateStrings : [];
  dateRange.value = [
    typeof values[0] === 'string' && values[0] ? values[0] : null,
    typeof values[1] === 'string' && values[1] ? values[1] : null,
  ];
}
function resetScroll() {
  if (tableViewport.value) tableViewport.value.scrollTop = 0;
}
function changePage(next: number, nextSize: number) {
  if (store.loading || busy.value) return;
  page.value = nextSize === pageSize.value ? next : 1;
  pageSize.value = nextSize;
}
async function refresh() {
  explicitRefreshes += 1;
  try {
    await Promise.all([store.refresh(), loadOptions()]);
    if (detailId.value) await loadDetail();
  } finally {
    explicitRefreshes -= 1;
  }
}
async function loadDetail() {
  const id = detailId.value;
  const version = ++detailVersion;
  detail.value = null;
  detailError.value = '';
  detailLoading.value = Boolean(id);
  if (!id) return;
  try {
    const run = await store.getRun(id);
    if (!disposed && version === detailVersion) {
      if (run.status === 'running')
        detailError.value = '这场测试正在运行，请前往智商测试查看看板。';
      else detail.value = run;
    }
  } catch (cause) {
    if (!disposed && version === detailVersion)
      detailError.value = cause instanceof Error ? cause.message : '运行详情读取失败';
  } finally {
    if (!disposed && version === detailVersion) detailLoading.value = false;
  }
}
function openDetail(run: IqRun) {
  void router.replace({ query: { ...route.query, runId: run.id } });
}
function closeDetail() {
  const query = { ...route.query };
  delete query.runId;
  void router.replace({ query });
}
async function remove(id: string) {
  if (busy.value) return;
  busy.value = true;
  explicitRefreshes += 1;
  try {
    await store.deleteRuns([id]);
    await loadOptions();
    if (detailId.value === id) closeDetail();
  } catch {
    /* API 已反馈写入失败，保留原列表。 */
  } finally {
    explicitRefreshes -= 1;
    busy.value = false;
  }
}
onMounted(() => {
  void store.connect();
  void refresh();
});
onScopeDispose(() => {
  disposed = true;
  detailVersion += 1;
});
</script>

<template>
  <div class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-6">
    <div
      class="mb-4 flex min-w-0 flex-none flex-wrap items-center gap-2 border-b border-rp-line pb-4"
    >
      <a-range-picker
        :value="dateRange"
        value-format="YYYY-MM-DD"
        format="YYYY-MM-DD"
        :presets="datePresets"
        :allow-empty="[true, true]"
        :placeholder="['开始日期', '结束日期']"
        aria-label="IQ 测试时间范围"
        class="w-full min-w-0 shrink-0 sm:w-80"
        @change="changeDateRange"
      />
      <a-cascader
        v-model:value="participantPaths"
        :options="options"
        multiple
        check-strictly
        max-tag-count="responsive"
        allow-clear
        :disabled="optionsLoading || Boolean(optionsError)"
        :status="optionsError ? 'error' : undefined"
        :placeholder="optionsLoading ? '站点 / 模型加载中…' : '全部站点 / 模型'"
        aria-label="IQ 站点和模型筛选"
        :styles="{ root: { width: '320px', maxWidth: '100%', flexShrink: 0 } }"
        class="min-w-0"
      >
        <template #optionRender="option">
          <span
            v-if="option.children?.length"
            v-site-model-selection="{ siteId: option.value, paths: participantPaths }"
            >{{ option.label }}</span
          >
          <span v-else>{{ option.label }}</span>
        </template>
      </a-cascader>
      <a-select
        v-model:value="result"
        aria-label="IQ 执行结果筛选"
        placeholder="全部执行结果"
        allow-clear
        class="w-full min-w-0 sm:w-40"
        :options="resultOptions"
      />
    </div>
    <a-alert
      v-if="optionsError"
      type="error"
      title="站点 / 模型筛选加载失败"
      :description="optionsError"
      show-icon
      class="mb-4 flex-none"
    >
      <template #action>
        <a-button size="small" aria-label="重试 IQ 筛选目录" @click="loadOptions">重试</a-button>
      </template>
    </a-alert>

    <section
      ref="tableViewport"
      aria-label="IQ 历史记录列表"
      :aria-busy="store.loading"
      class="min-h-0 min-w-0 flex-1 overflow-auto"
    >
      <a-alert
        v-if="store.error"
        type="error"
        title="历史记录读取失败"
        :description="store.error"
        show-icon
      >
        <template #action><a-button size="small" @click="refresh">重新加载</a-button></template>
      </a-alert>
      <a-table
        v-else
        :columns="columns"
        :data-source="visibleRuns"
        :pagination="false"
        :loading="store.loading"
        table-layout="fixed"
        :scroll="{ x: 1000 }"
        row-key="id"
        size="small"
      >
        <template #emptyText
          ><a-empty :description="hasFilters ? '没有符合条件的测试记录' : '还没有测试记录'"
        /></template>
        <template #bodyCell="{ column, record }">
          <span v-if="column.key === 'startedAtMs'" class="text-rp-secondary tabular-nums">{{
            formatIqTime(record.startedAtMs)
          }}</span>
          <a-tooltip v-else-if="column.key === 'questions'" placement="topLeft">
            <template #title>
              <ol class="m-0 max-h-80 max-w-lg list-none space-y-3 overflow-auto p-0">
                <li
                  v-for="(question, index) in record.questions"
                  :key="question.id"
                  class="whitespace-pre-wrap wrap-break-word"
                >
                  {{ index + 1 }}. {{ question.content }}
                </li>
              </ol>
            </template>
            <div class="min-w-0">
              <div class="truncate">{{ record.questions[0]?.content || '—' }}</div>
              <div class="mt-1 text-xs text-rp-tertiary">
                共 {{ record.summary.totalQuestions }} 题
              </div>
            </div>
          </a-tooltip>
          <a-tooltip v-else-if="column.key === 'participants'" placement="topLeft">
            <template #title>
              <ul class="m-0 max-h-80 max-w-lg list-none space-y-2 overflow-auto p-0">
                <li
                  v-for="participant in record.participants"
                  :key="participant.id"
                  class="wrap-break-word"
                >
                  <div>{{ participant.modelName }}</div>
                  <div>{{ participant.siteName }} · {{ formatIqStrength(participant) }}</div>
                </li>
              </ul>
            </template>
            <div class="flex min-w-0 items-center gap-1.5">
              <span class="truncate">{{ record.participants[0]?.modelName || '—' }}</span>
              <span
                v-if="record.summary.totalParticipants > 1"
                class="shrink-0 text-xs text-rp-tertiary"
              >
                等 {{ record.summary.totalParticipants }} 位
              </span>
            </div>
          </a-tooltip>
          <a-tag v-else-if="column.key === 'result'" :color="record.callResult.color">{{
            record.callResult.label
          }}</a-tag>
          <span v-else-if="column.key === 'durationMs'" class="text-rp-secondary tabular-nums">{{
            formatIqDuration(record.summary.durationMs)
          }}</span>
          <div v-else-if="column.key === 'actions'" class="flex justify-center gap-1">
            <a-tooltip title="运行详情">
              <a-button
                color="primary"
                variant="text"
                shape="circle"
                :aria-label="`查看运行 ${record.id} 的详情`"
                @click="openDetail(record)"
              >
                <template #icon><AppIcon name="FileSearchOutlined" /></template>
              </a-button>
            </a-tooltip>
            <a-popconfirm
              title="删除这次测试记录？"
              description="将同时删除原始流文件与产物，不可恢复。"
              ok-text="删除"
              cancel-text="取消"
              ok-type="danger"
              :disabled="busy"
              @confirm="remove(record.id)"
            >
              <a-button
                type="text"
                shape="circle"
                danger
                :disabled="busy"
                :aria-label="`删除运行 ${record.id}`"
              >
                <template #icon><AppIcon name="DeleteOutlined" /></template>
              </a-button>
            </a-popconfirm>
          </div>
        </template>
      </a-table>
    </section>
    <div
      v-if="!store.error"
      class="flex min-w-0 flex-none flex-wrap items-center gap-2 pt-4 sm:justify-end"
    >
      <div class="min-w-0 max-w-full overflow-x-auto pb-1">
        <a-pagination
          :current="page"
          :page-size="pageSize"
          :page-size-options="['10', '20', '50', '100']"
          show-size-changer
          :show-quick-jumper="false"
          :show-total="showTotal"
          size="small"
          :total="filtered.length"
          :disabled="store.loading || busy"
          @change="changePage"
        />
      </div>
    </div>

    <a-drawer
      :open="Boolean(detailId)"
      :size="620"
      :closable="{ placement: 'start', 'aria-label': '关闭运行详情' }"
      :loading="detailLoading"
      :styles="{
        wrapper: { maxWidth: '100vw' },
        header: { padding: '12px 20px' },
        body: { padding: '16px 20px' },
      }"
      @close="closeDetail"
    >
      <template #title>
        <div class="min-w-0">
          <span class="text-base font-semibold">运行详情</span>
          <div v-if="detail" class="mt-0.5 text-xs font-normal text-rp-tertiary">
            {{ formatIqTime(detail.startedAtMs) }} · {{ detail.summary.totalQuestions }} 道题目 ·
            {{ detail.summary.totalParticipants }} 位参与者
          </div>
        </div>
      </template>
      <a-alert
        v-if="detailError"
        title="无法读取运行详情"
        :description="detailError"
        type="error"
        show-icon
      >
        <template #action><a-button size="small" @click="loadDetail">重试</a-button></template>
      </a-alert>
      <IqRunDetail v-else-if="detail" :run="detail" source="history" />
    </a-drawer>
  </div>
</template>
