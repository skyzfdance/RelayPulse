<script setup lang="ts">
import type { ProbeRecordListItem, ProbeResult, RecordTimeRange } from '@/types/domain/records';
import type { MenuItemType } from 'antdv-next';

import { DeleteOutlined } from '@antdv-next/icons';
import { computed, onMounted, onScopeDispose, ref, shallowRef } from 'vue';

import { onMainWindowRefresh } from '@/composables/useMainWindowNavigation';

import { rangeOptions, resultOptions, USAGE_EXPLANATION } from './records-presentation';
import { useProbeRecordOptions } from './use-probe-record-options';
import { useProbeRecords } from './use-probe-records';
import { useTokenStats } from './use-token-stats';

import ProbeRecordDetailDrawer from './components/ProbeRecordDetailDrawer.vue';
import ProbeRecordsTable from './components/ProbeRecordsTable.vue';
import TokenStatsTable from './components/TokenStatsTable.vue';
import AppIcon from '@/components/AppIcon.vue';

const tab = ref<'records' | 'tokens'>('records');
const range = ref<RecordTimeRange>('24h');
const siteId = ref('');
const recordList = useProbeRecords({ range, siteId });
const tokenList = useTokenStats({ range, siteId });
const filterOptions = useProbeRecordOptions(siteId);
const {
  data: recordData,
  loading: recordsLoading,
  error: recordsError,
  modelName,
  result,
  paging: recordPaging,
  busy,
} = recordList;
const {
  data: tokenData,
  loading: tokensLoading,
  error: tokensError,
  paging: tokenPaging,
} = tokenList;
const {
  data: options,
  loading: optionsLoading,
  error: optionsError,
  siteOptions,
  modelOptions,
} = filterOptions;
const detailRecordId = ref<string | null>(null);
const deleteTarget = shallowRef<ProbeRecordListItem | 'all' | null>(null);
const loading = computed(() => optionsLoading.value || recordsLoading.value || tokensLoading.value);
const retainedCount = computed(
  () => recordData.value?.retainedCount ?? tokenData.value?.retainedCount ?? 0,
);
const currentError = computed(() =>
  tab.value === 'records' ? recordsError.value : tokensError.value,
);
let disposed = false;
const tabs = [
  { value: 'records', label: '探测记录' },
  { value: 'tokens', label: 'Token 统计' },
];
const menuItems = computed<MenuItemType[]>(() => [
  {
    key: 'clear',
    label: '清理全部记录',
    icon: DeleteOutlined,
    danger: true,
    disabled: busy.value || loading.value || retainedCount.value === 0,
  },
]);
const clearAll = computed(() => deleteTarget.value === 'all');

function loadLists() {
  return Promise.all([recordList.load(), tokenList.load()]);
}
async function refresh() {
  if (disposed || busy.value) return;
  await Promise.all([filterOptions.load(), loadLists()]);
}
function changeRange(value: RecordTimeRange) {
  range.value = value;
  recordPaging.page = tokenPaging.page = 1;
  void loadLists();
}
function changeSite(value: string) {
  siteId.value = value;
  if (!modelOptions.value.some((option) => option.value === modelName.value)) modelName.value = '';
  recordPaging.page = tokenPaging.page = 1;
  void loadLists();
}
function resetFilters() {
  range.value = '24h';
  siteId.value = '';
  if (tab.value === 'records') modelName.value = result.value = '';
  recordPaging.page = tokenPaging.page = 1;
  void loadLists();
}
function requestDelete(record: ProbeRecordListItem) {
  if (!busy.value && !recordsLoading.value) deleteTarget.value = record;
}
function requestClear() {
  if (!busy.value && !loading.value && retainedCount.value > 0) deleteTarget.value = 'all';
}
async function confirmDelete() {
  const target = deleteTarget.value;
  if (!target || busy.value) return;
  const success =
    target === 'all' ? await recordList.clearRecords() : await recordList.removeRecord(target.id);
  if (!success || disposed) return;
  // 两张表分别重新查询后端，不在前端删除行、扣减 Token 或重算合计。
  tokenList.invalidate(true);
  if (target === 'all') recordPaging.page = tokenPaging.page = 1;
  if (target === 'all' || detailRecordId.value === target.id) detailRecordId.value = null;
  deleteTarget.value = null;
  await refresh();
}
onMounted(refresh);
onMainWindowRefresh('/run/records', () => {
  tab.value = 'records';
  return refresh();
});
onScopeDispose(() => {
  disposed = true;
});
</script>
<template>
  <div class="min-w-0 px-6 py-5">
    <div class="mb-4 flex flex-wrap items-center gap-2">
      <a-segmented v-model:value="tab" :options="tabs" aria-label="探测记录视图" />
      <a-tooltip
        v-if="tab === 'tokens'"
        :title="USAGE_EXPLANATION"
        :trigger="['hover', 'focus']"
        placement="bottomLeft"
        :styles="{ container: { maxWidth: '380px' } }"
      >
        <a-button type="text" shape="circle" aria-label="Token 统计口径说明">
          <template #icon
            ><AppIcon name="QuestionCircleOutlined" class="text-rp-tertiary"
          /></template>
        </a-button>
      </a-tooltip>
      <div class="ml-auto flex items-center gap-2">
        <a-tooltip title="刷新列表">
          <a-button
            shape="circle"
            :loading="loading"
            :disabled="busy"
            aria-label="刷新探测记录和 Token 统计"
            @click="refresh()"
          >
            <template #icon><AppIcon name="ReloadOutlined" /></template>
          </a-button>
        </a-tooltip>
        <a-dropdown
          :menu="{ items: menuItems }"
          :trigger="['click']"
          placement="bottomRight"
          :disabled="busy || loading"
          @menu-click="requestClear()"
        >
          <a-button shape="circle" :disabled="busy || loading" aria-label="更多探测记录操作">
            <template #icon><AppIcon name="MoreOutlined" /></template>
          </a-button>
        </a-dropdown>
      </div>
    </div>
    <a-alert
      v-if="optionsError"
      class="mb-4"
      type="error"
      show-icon
      title="筛选选项加载失败"
      :description="optionsError"
    >
      <template #action
        ><a-button size="small" :loading="optionsLoading" @click="filterOptions.load()"
          >重试</a-button
        ></template
      >
    </a-alert>
    <div class="mb-4 flex flex-wrap gap-2 border-b border-rp-line pb-4">
      <a-select
        :value="range"
        class="w-44"
        aria-label="时间范围"
        :options="rangeOptions"
        :disabled="busy"
        @change="changeRange($event as RecordTimeRange)"
      />
      <a-select
        :value="siteId"
        class="w-44"
        aria-label="站点筛选"
        :options="siteOptions"
        show-search
        option-filter-prop="label"
        :disabled="busy || !options"
        @change="changeSite($event as string)"
      />
      <template v-if="tab === 'records'">
        <a-select
          :value="modelName"
          class="w-48"
          aria-label="模型筛选"
          :options="modelOptions"
          show-search
          option-filter-prop="label"
          :disabled="busy || !options"
          @change="recordList.changeModel($event as string)"
        />
        <a-select
          :value="result"
          class="w-40"
          aria-label="探测结果筛选"
          :options="resultOptions"
          :disabled="busy"
          @change="recordList.changeResult($event as ProbeResult | '')"
        />
      </template>
    </div>
    <a-alert
      v-if="currentError"
      type="error"
      show-icon
      :title="tab === 'records' ? '探测记录加载失败' : 'Token 统计加载失败'"
      :description="currentError"
      role="alert"
    >
      <template #action>
        <a-button
          :loading="tab === 'records' ? recordsLoading : tokensLoading"
          @click="tab === 'records' ? recordList.load() : tokenList.load()"
          >重新加载</a-button
        >
      </template>
    </a-alert>
    <ProbeRecordsTable
      v-else-if="tab === 'records'"
      :data="recordData"
      :loading="recordsLoading"
      :busy="busy"
      v-bind="recordPaging"
      @paginate="recordList.paginate"
      @reset="resetFilters"
      @detail="detailRecordId = $event.id"
      @remove="requestDelete"
    />
    <TokenStatsTable
      v-else
      :data="tokenData"
      :loading="tokensLoading"
      :busy="busy"
      v-bind="tokenPaging"
      @paginate="tokenList.paginate"
      @reset="resetFilters"
    />
    <ProbeRecordDetailDrawer :record-id="detailRecordId" @close="detailRecordId = null" />
    <a-modal
      :open="deleteTarget !== null"
      :title="clearAll ? '清理全部探测记录？' : '删除这条探测记录？'"
      :ok-text="clearAll ? '清理全部记录' : '删除记录'"
      cancel-text="取消"
      :ok-button-props="{ danger: true }"
      :cancel-button-props="{ disabled: busy }"
      :confirm-loading="busy"
      :closable="!busy"
      :keyboard="!busy"
      :mask="{ closable: !busy }"
      @ok="confirmDelete()"
      @cancel="!busy && (deleteTarget = null)"
    >
      <template v-if="clearAll">
        <p>
          将清理全部站点、全部时间范围的正式探测记录，包括当前筛选之外的记录，同步删除对应的缓存文件，并清空对应的
          Token 统计。
        </p>
        <p class="m-0! text-rp-secondary">当前健康状态及站点、模型配置会保留。删除后无法恢复。</p>
      </template>
      <p v-else-if="deleteTarget" class="m-0!">
        删除后无法恢复。删除记录时会同步删除对应的缓存文件，并更新对应的 Token
        统计。不同步修改对应模型健康状态。
      </p>
    </a-modal>
  </div>
</template>
