<script setup lang="ts">
import type { ProbeRecordRequestSummary, ProbeRecordResponseSummary } from '@/types/domain/records';
import type { CollapseProps } from 'antdv-next';

import { computed, ref, watch } from 'vue';

import { formatBytes } from '@/utils/format-bytes';

import { useProbeRecordRequest } from '../use-probe-record-request';
import { useProbeRecordResponse } from '../use-probe-record-response';

import ProbeRecordResponsePanel from './ProbeRecordResponsePanel.vue';

const props = defineProps<{
  recordId: string;
  summary: ProbeRecordRequestSummary;
  responseSummary: ProbeRecordResponseSummary;
}>();
const activeKeys = ref<string[]>([]);
const panels: CollapseProps['items'] = [
  { key: 'request', label: '请求详情' },
  { key: 'response', label: '原始返回' },
];
watch(
  () => props.recordId,
  () => {
    activeKeys.value = [];
  },
  { flush: 'sync' },
);
const { content, loading, error, retryable, reload } = useProbeRecordRequest(
  () => props.recordId,
  () => activeKeys.value.includes('request'),
);
const {
  content: response,
  loading: responseLoading,
  error: responseError,
  retryable: responseRetryable,
  reload: reloadResponse,
  openingDirectory,
  openLogDirectory,
} = useProbeRecordResponse(
  () => props.recordId,
  () => activeKeys.value.includes('response'),
);
const responseSummary = computed(() => response.value ?? props.responseSummary);
const responseKindLabels = { stream: '流式', json: 'JSON', other: '其他' } as const;
const summaryLabel = computed(() => {
  const summary = content.value ?? props.summary;
  return `${summary?.method ?? '—'} · ${summary?.bodyBytes == null ? '—' : formatBytes(summary.bodyBytes)}`;
});
</script>

<template>
  <section aria-label="请求与响应" class="pt-4">
    <h3 class="m-0! mb-3! text-sm font-semibold">请求与响应</h3>
    <a-collapse
      v-model:active-key="activeKeys"
      :items="panels"
      expand-icon-placement="end"
      size="small"
    >
      <template #labelRender="{ item }">
        <span class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
          <span class="text-sm">{{ item.label }}</span>
          <span
            v-if="item.key === 'request'"
            class="text-xs text-rp-secondary tabular-nums"
            title="请求体快照的 UTF-8 字节数"
          >
            {{ summaryLabel }}
          </span>
          <span v-else class="flex items-center gap-2 text-xs text-rp-secondary tabular-nums">
            <span v-if="responseSummary.state === 'not_received'">未收到响应</span>
            <a-tag
              v-else-if="responseSummary.kind"
              :color="responseSummary.kind === 'stream' ? 'blue' : undefined"
              class="m-0!"
            >
              {{ responseKindLabels[responseSummary.kind] }}
            </a-tag>
            <span title="实际接收的响应正文大小">
              {{
                responseSummary.receivedBytes === null
                  ? '—'
                  : formatBytes(responseSummary.receivedBytes)
              }}
            </span>
          </span>
        </span>
      </template>
      <template #contentRender="{ item }">
        <section v-if="item.key === 'request'" aria-label="请求详情内容" :aria-busy="loading">
          <div v-if="loading" role="status" aria-label="请求详情加载中">
            <a-skeleton active :title="false" :paragraph="{ rows: 3 }" />
          </div>
          <a-alert
            v-else-if="error"
            type="error"
            show-icon
            title="请求详情加载失败"
            :description="error"
            role="alert"
          >
            <template v-if="retryable" #action>
              <a-button size="small" @click="reload">重新加载</a-button>
            </template>
          </a-alert>
          <template v-else-if="content">
            <h4 class="m-0! mb-2! text-xs font-medium text-rp-secondary">
              请求头（敏感字段已脱敏）
            </h4>
            <p v-if="content.headers === null" class="m-0! text-xs text-rp-tertiary">
              未保存请求头
            </p>
            <p v-else-if="!content.headers.length" class="m-0! text-xs text-rp-tertiary">
              无请求头
            </p>
            <dl v-else aria-label="已脱敏请求头" class="m-0!">
              <div
                v-for="(header, index) in content.headers"
                :key="index"
                class="grid grid-cols-[112px_minmax(0,1fr)] gap-x-3 border-b border-rp-line py-2 last:border-0 max-[480px]:grid-cols-1 max-[480px]:gap-y-1"
              >
                <dt class="text-xs leading-5 wrap-anywhere text-rp-secondary">
                  {{ header.name }}
                </dt>
                <dd class="m-0! font-mono text-xs leading-5 wrap-anywhere">
                  {{ header.value }}
                </dd>
              </div>
            </dl>
            <h4 class="m-0! mt-4! mb-2! text-xs font-medium text-rp-secondary">请求体快照</h4>
            <p v-if="content.body === null" class="m-0! text-xs text-rp-tertiary">未保存请求体</p>
            <p v-else-if="content.body === ''" class="m-0! text-xs text-rp-tertiary">请求体为空</p>
            <pre
              v-else
              aria-label="请求体快照"
              tabindex="0"
              class="m-0! max-h-64 overflow-auto rounded-md bg-rp-subtle p-3 font-mono text-xs! leading-5! whitespace-pre-wrap wrap-anywhere"
            ><code>{{ content.body }}</code></pre>
          </template>
        </section>
        <ProbeRecordResponsePanel
          v-else-if="item.key === 'response'"
          :content="response"
          :loading="responseLoading"
          :error="responseError"
          :retryable="responseRetryable"
          :opening-directory="openingDirectory"
          @reload="reloadResponse"
          @open-log-directory="openLogDirectory"
        />
      </template>
    </a-collapse>
  </section>
</template>
