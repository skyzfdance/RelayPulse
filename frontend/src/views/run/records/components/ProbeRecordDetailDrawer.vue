<script setup lang="ts">
import { Empty } from 'antdv-next';
import { computed } from 'vue';

import {
  formatCount,
  formatDuration,
  formatRecordTime,
  resultColors,
  resultLabels,
} from '../records-presentation';
import { useProbeRecordDetail } from '../use-probe-record-detail';

import ProbeRecordRequestSection from './ProbeRecordRequestSection.vue';

const props = defineProps<{ recordId: string | null }>();
const emit = defineEmits<{ close: [] }>();
const { detail, loading, error, notFound, retryable, reload } = useProbeRecordDetail(
  () => props.recordId,
);
const comparisonLabels = { gte: '≥', lt: '<' } as const;
const slowChecks = computed(() => {
  const record = detail.value;
  if (!record?.slowThresholds || !record.slowEvaluation) return [];
  return [
    {
      label: '首 Token',
      actual: record.firstTokenMs,
      threshold: record.slowThresholds.firstTokenMs,
      ...record.slowEvaluation.firstToken,
    },
    {
      label: '总耗时',
      actual: record.durationMs,
      threshold: record.slowThresholds.durationMs,
      ...record.slowEvaluation.duration,
    },
  ];
});
/** 判定依据保留毫秒精度，避免四舍五入后实测值和阈值看起来相等。 */
function formatCriterionDuration(value: number | null) {
  if (value === null) return '—';
  return value < 1000 ? `${value} ms` : `${value / 1000} s`;
}
const metrics = computed(() => {
  const record = detail.value;
  if (!record) return [];
  return [
    { label: '首 Token', value: formatDuration(record.firstTokenMs) },
    { label: '总耗时', value: formatDuration(record.durationMs) },
    { label: '输入 Token', value: formatCount(record.usage?.inputTokens) },
    { label: '输出 Token', value: formatCount(record.usage?.outputTokens) },
    { label: '总 Token', value: formatCount(record.usage?.totalTokens) },
  ];
});
const requestFields = computed(() => {
  const info = detail.value?.requestInfo;
  if (!info) return [];
  return [
    { label: '请求端点', value: info.endpoint ?? '—', mono: true },
    { label: '探针策略', value: info.strategyLabel ?? '—', mono: false },
    { label: 'HTTP 状态', value: info.httpStatus?.toString() ?? '—', mono: false },
    { label: '请求 ID', value: info.requestId ?? '—', mono: true },
  ];
});
</script>

<template>
  <a-drawer
    :open="recordId !== null"
    :size="640"
    :closable="{ placement: 'start', 'aria-label': '关闭探测详情' }"
    :styles="{
      wrapper: { maxWidth: '100vw' },
      header: { padding: '12px 20px' },
      body: { padding: '16px 20px' },
    }"
    destroy-on-hidden
    @close="emit('close')"
  >
    <template #title>
      <div class="min-w-0">
        <span class="text-base font-semibold">探测详情</span>
        <div
          v-if="detail"
          class="mt-0.5 truncate text-xs font-normal text-rp-secondary"
          :title="`${detail.modelName} · ${detail.siteName}`"
        >
          {{ detail.modelName }} · {{ detail.siteName }}
        </div>
      </div>
    </template>
    <section aria-label="探测详情内容" :aria-busy="loading">
      <div v-if="loading" role="status" aria-label="探测详情加载中">
        <a-skeleton active :paragraph="{ rows: 8 }" />
      </div>
      <a-empty v-else-if="notFound" class="py-12" :image="Empty.PRESENTED_IMAGE_SIMPLE">
        <template #description>
          <p class="mb-1! text-rp-secondary">这条探测记录已不存在或已过期</p>
          <p class="m-0! text-xs text-rp-tertiary">请关闭详情后刷新列表。</p>
        </template>
      </a-empty>
      <a-alert
        v-else-if="error"
        type="error"
        show-icon
        title="探测详情加载失败"
        :description="error"
        role="alert"
      >
        <template v-if="retryable" #action>
          <a-button size="small" @click="reload">重新加载</a-button>
        </template>
      </a-alert>
      <template v-else-if="detail">
        <div class="border-b border-rp-line pb-3">
          <div class="flex flex-wrap items-start justify-between gap-x-4 gap-y-1">
            <span
              class="inline-flex min-w-0 items-start gap-2 text-sm font-medium wrap-anywhere"
              :class="{ 'text-(--ant-color-error)': detail.result === 'failure' }"
              aria-label="本次请求结果"
              title="仅表示本次探测请求的结果，与当前模型健康状态分开记录。"
            >
              <span
                class="mt-1.75 size-1.5 shrink-0 rounded-full"
                :style="{ background: resultColors[detail.result] }"
              />
              {{ detail.resultMessage.trim() || resultLabels[detail.result] }}
            </span>
            <time
              :datetime="detail.probedAt"
              class="shrink-0 text-xs leading-5 whitespace-nowrap text-rp-secondary tabular-nums"
              >{{ formatRecordTime(detail.probedAt, true) }}</time
            >
          </div>
          <div aria-label="当次偏慢判定依据" class="mt-2 text-xs leading-5">
            <template v-if="detail.slowThresholds && detail.slowEvaluation">
              <p class="m-0! text-rp-tertiary">
                {{
                  detail.slowEvaluation.status === 'skipped_failure'
                    ? '当次慢阈值（请求失败，未参与偏慢判定）'
                    : '判定依据（当次快照，任一达到阈值）'
                }}
              </p>
              <div class="flex flex-wrap gap-x-4 text-rp-secondary tabular-nums">
                <span
                  v-for="check in slowChecks"
                  :key="check.label"
                  :class="{ 'text-(--ant-color-warning)': check.triggered }"
                >
                  {{ check.label }}
                  <template v-if="detail.slowEvaluation.status === 'skipped_failure'">
                    阈值 {{ formatCriterionDuration(check.threshold) }}
                  </template>
                  <template v-else>
                    {{ formatCriterionDuration(check.actual) }}
                    <template v-if="check.comparison">
                      {{ comparisonLabels[check.comparison] }}
                      {{ formatCriterionDuration(check.threshold) }}（阈值）
                    </template>
                    <template v-else>
                      · 阈值 {{ formatCriterionDuration(check.threshold) }}
                    </template>
                  </template>
                </span>
              </div>
            </template>
            <p v-else class="m-0! text-rp-tertiary">未保存当次判定快照</p>
          </div>
        </div>

        <section aria-label="性能与用量" class="border-b border-rp-line py-4">
          <h3 class="m-0! mb-3! text-sm font-semibold">性能与用量</h3>
          <dl class="m-0! grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
            <div v-for="metric in metrics" :key="metric.label" class="min-w-0">
              <dt class="text-xs text-rp-secondary">{{ metric.label }}</dt>
              <dd class="m-0! mt-1! text-sm font-medium tabular-nums">{{ metric.value }}</dd>
            </div>
            <div>
              <dt class="text-xs text-rp-secondary">用量来源</dt>
              <dd class="m-0! mt-1!">
                <a-tag :color="detail.usageSource === 'provider' ? 'blue' : undefined" class="m-0!">
                  {{ detail.usageSource === 'provider' ? '供应商上报' : '未上报' }}
                </a-tag>
              </dd>
            </div>
          </dl>
        </section>

        <section aria-label="请求信息" class="border-b border-rp-line pt-4">
          <h3 class="m-0! mb-2! text-sm font-semibold">请求信息</h3>
          <dl class="m-0!">
            <div
              v-for="field in requestFields"
              :key="field.label"
              class="grid grid-cols-[80px_minmax(0,1fr)] gap-x-3 border-b border-rp-line py-2 last:border-0"
            >
              <dt class="text-xs leading-5 text-rp-secondary">{{ field.label }}</dt>
              <dd class="m-0! text-xs leading-5 wrap-anywhere" :class="{ 'font-mono': field.mono }">
                {{ field.value }}
              </dd>
            </div>
          </dl>
        </section>
        <ProbeRecordRequestSection
          :key="detail.id"
          :record-id="detail.id"
          :summary="detail.requestSummary"
          :response-summary="detail.responseSummary"
        />
      </template>
    </section>
  </a-drawer>
</template>
