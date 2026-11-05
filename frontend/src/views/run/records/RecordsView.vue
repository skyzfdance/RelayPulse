<script setup lang="ts">
import type {
  ProbeRecordAssociationLookup,
  ProbeRecordListItem,
  ProbeResult,
  RecordsPage,
  RecordTimeRange,
} from '@/types/domain/records';
import type { MenuItemType } from 'antdv-next';

import { DeleteOutlined } from '@antdv-next/icons';
import { computed, onMounted, onScopeDispose, reactive, ref, shallowRef, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { fetchProbeRecordsByIds } from '@/api/run/records';
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
const associationRecordIds = ref<string[]>([]);
const hasAssociation = computed(() => associationRecordIds.value.length > 0);
const recordList = useProbeRecords(
  { range, siteId },
  () => tab.value === 'records' && !hasAssociation.value,
);
const tokenList = useTokenStats(
  { range, siteId },
  () => tab.value === 'tokens' && !hasAssociation.value,
);
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
const association = shallowRef<ProbeRecordAssociationLookup | null>(null);
const associationPaging = reactive({ page: 1, pageSize: 20 });
const associationLoading = ref(false);
const associationError = ref('');
const returnToOperationHistory = ref(false);
const route = useRoute();
const router = useRouter();
const deleteTarget = shallowRef<ProbeRecordListItem | 'all' | null>(null);
const loading = computed(
  () =>
    associationLoading.value || optionsLoading.value || recordsLoading.value || tokensLoading.value,
);
const retainedCount = computed(
  () => recordData.value?.retainedCount ?? tokenData.value?.retainedCount ?? 0,
);
const associationPage = computed<RecordsPage<ProbeRecordListItem> | undefined>(() => {
  if (!association.value) return undefined;
  return {
    items: association.value.items.slice(
      (associationPaging.page - 1) * associationPaging.pageSize,
      associationPaging.page * associationPaging.pageSize,
    ),
    total: association.value.items.length,
    page: associationPaging.page,
    retainedCount: association.value.items.length,
  };
});
const currentError = computed(() => {
  if (hasAssociation.value) return associationError.value;
  return tab.value === 'records' ? recordsError.value : tokensError.value;
});
let disposed = false;
let associationVersion = 0;
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
async function resolveAssociatedRecords() {
  const idsValue = route?.query.recordIds;
  const idsText = Array.isArray(idsValue) ? idsValue[0] : idsValue;
  if (typeof idsText === 'string' && idsText.trim()) {
    tab.value = 'records';
    detailRecordId.value = null;
    association.value = null;
    associationPaging.page = 1;
    associationRecordIds.value = [
      ...new Set(
        idsText
          .split(',')
          .map((id) => id.trim())
          .filter(Boolean),
      ),
    ];
  }
  const recordIds = associationRecordIds.value;
  if (!recordIds.length) return;
  const requestVersion = ++associationVersion;
  returnToOperationHistory.value = route?.query.fromOperationHistory === '1';
  associationLoading.value = true;
  associationError.value = '';
  try {
    const result = await fetchProbeRecordsByIds(recordIds);
    if (disposed || requestVersion !== associationVersion) return;
    association.value = result;
    associationPaging.page = Math.min(
      associationPaging.page,
      Math.max(1, Math.ceil(result.items.length / associationPaging.pageSize)),
    );
    if (recordIds.length === 1 && result.items[0]) detailRecordId.value = result.items[0].id;
  } catch (cause) {
    if (!disposed && requestVersion === associationVersion) {
      associationError.value = cause instanceof Error ? cause.message : '无法读取关联的探测记录';
      reportOperationReadFailure('probe_records', 'page', cause);
    }
  } finally {
    if (!disposed && requestVersion === associationVersion) associationLoading.value = false;
    const query = { ...(route?.query ?? {}) };
    delete query.recordIds;
    if (!disposed && requestVersion === associationVersion && router && route?.query.recordIds)
      void router.replace({ query });
  }
}
function returnToOperationLog() {
  if (router) router.back();
}
function exitAssociation() {
  associationVersion += 1;
  associationRecordIds.value = [];
  association.value = null;
  associationLoading.value = false;
  associationError.value = '';
  detailRecordId.value = null;
  void refresh();
}
function paginateRecords(page: number, pageSize: number) {
  if (!hasAssociation.value) {
    recordList.paginate(page, pageSize);
    return;
  }
  associationPaging.page = pageSize === associationPaging.pageSize ? page : 1;
  associationPaging.pageSize = pageSize;
}
async function refresh() {
  if (disposed || busy.value) return;
  if (hasAssociation.value) {
    await resolveAssociatedRecords();
    return;
  }
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
onMounted(() => {
  if (route?.query.recordIds) void resolveAssociatedRecords();
  else void refresh();
});
watch(
  () => route?.query.recordIds,
  (recordIds) => {
    if (recordIds) void resolveAssociatedRecords();
  },
);
onMainWindowRefresh('/run/records', () => {
  tab.value = 'records';
  return refresh();
});
onScopeDispose(() => {
  disposed = true;
  associationVersion += 1;
});
</script>
<template>
  <div class="flex h-full min-w-0 flex-col overflow-hidden p-6">
    <div class="mb-4 flex flex-wrap items-center gap-2">
      <a-segmented
        v-if="!hasAssociation"
        v-model:value="tab"
        :options="tabs"
        aria-label="探测记录视图"
      />
      <a-button v-else @click="exitAssociation">查看全部探测记录</a-button>
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
      <a-button v-if="returnToOperationHistory" @click="returnToOperationLog">
        <template #icon><AppIcon name="ArrowRightOutlined" class="rotate-180" /></template>
        返回操作记录
      </a-button>
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
          v-if="!hasAssociation"
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
      v-if="association && (association.items.length || association.missingIds.length)"
      class="mb-3"
      :type="association.missingIds.length ? 'warning' : 'info'"
      show-icon
      :title="association.missingIds.length ? '关联探测记录已不可用' : '本次探测关联记录'"
      :description="
        association.missingIds.length
          ? `已定位 ${association.items.length} 条记录；${association.missingIds.length} 条关联记录已过期或清理。`
          : `已按事件关联 ID 定位 ${association.items.length} 条探测记录。`
      "
      role="status"
    />
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
    <div
      v-if="!hasAssociation"
      class="mb-4 flex min-w-0 flex-wrap gap-2 border-b border-rp-line pb-4"
    >
      <a-select
        :value="range"
        class="min-w-0 w-full sm:w-44"
        aria-label="时间范围"
        :options="rangeOptions"
        :disabled="busy"
        @change="changeRange($event as RecordTimeRange)"
      />
      <a-select
        :value="siteId"
        class="min-w-0 w-full sm:w-44"
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
          class="min-w-0 w-full sm:w-48"
          aria-label="模型筛选"
          :options="modelOptions"
          show-search
          option-filter-prop="label"
          :disabled="busy || !options"
          @change="recordList.changeModel($event as string)"
        />
        <a-select
          :value="result"
          class="min-w-0 w-full sm:w-40"
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
      :title="
        hasAssociation
          ? '关联探测记录读取失败'
          : tab === 'records'
            ? '探测记录加载失败'
            : 'Token 统计加载失败'
      "
      :description="currentError"
      role="alert"
    >
      <template #action>
        <a-button
          :loading="
            hasAssociation ? associationLoading : tab === 'records' ? recordsLoading : tokensLoading
          "
          @click="
            hasAssociation
              ? resolveAssociatedRecords()
              : tab === 'records'
                ? recordList.load()
                : tokenList.load()
          "
          >重新加载</a-button
        >
      </template>
    </a-alert>
    <div v-else class="min-h-0 flex-1 overflow-hidden">
      <ProbeRecordsTable
        v-if="tab === 'records'"
        :data="hasAssociation ? associationPage : recordData"
        :loading="recordsLoading || associationLoading"
        :busy="busy"
        v-bind="hasAssociation ? associationPaging : recordPaging"
        @paginate="paginateRecords"
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
    </div>
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
