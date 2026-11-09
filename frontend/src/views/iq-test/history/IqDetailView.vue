<script setup lang="ts">
import type { IqEntry, IqRun } from '@/types/domain/iq-test';
import type { DescriptionsProps } from 'antdv-next';

import { computed, onScopeDispose, ref, shallowRef, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { useIqTestStore } from '@/stores/iq-test';

import { formatIqStrength, formatIqTime, iqStatusColors, iqStatusLabels } from '../iq-presentation';

import AppIcon from '@/components/AppIcon.vue';

const route = useRoute();
const router = useRouter();
const store = useIqTestStore();
const run = shallowRef<IqRun | null>(null);
const entry = shallowRef<IqEntry | null>(null);
const loading = ref(false);
const opening = ref(false);
const error = ref('');
let requestVersion = 0;
let disposed = false;
const fromHistory = computed(() => route.query.from === 'history');
const sourceName = computed(() => (fromHistory.value ? '历史记录' : '运行看板'));
const participant = computed(() =>
  entry.value ? run.value?.participants[entry.value.participantIndex] : undefined,
);
const question = computed(() =>
  entry.value ? run.value?.questions[entry.value.questionIndex] : undefined,
);
const snapshotItems = computed<DescriptionsProps['items']>(() => {
  const snapshot = participant.value;
  if (!snapshot) return [];
  return [
    { key: 'model', label: '模型', content: snapshot.modelName },
    { key: 'site', label: '站点', content: snapshot.siteName },
    { key: 'endpoint', label: '连接地址', content: snapshot.endpoint, span: 'filled' },
    { key: 'createdAt', label: '建立时间', content: formatIqTime(snapshot.createdAtMs) },
    { key: 'strength', label: 'IQ 推理强度', content: formatIqStrength(snapshot) },
    { key: 'parameters', label: 'IQ 请求参数', span: 'filled' },
  ];
});
const parameterJson = computed(() => JSON.stringify(participant.value?.parameters ?? {}, null, 2));

async function load() {
  const version = ++requestVersion;
  run.value = null;
  entry.value = null;
  error.value = '';
  loading.value = true;
  try {
    const id = typeof route.params.id === 'string' ? route.params.id : '';
    const runId = typeof route.query.runId === 'string' ? route.query.runId : '';
    if (!id || !runId) throw new Error('缺少调用或运行标识，请从运行看板或历史记录重新进入。');
    const result = await store.getRun(runId);
    const found = result.entries.find((item) => item.id === id && item.runId === runId);
    if (!found) throw new Error('这条调用不存在或不属于当前运行，可能已被删除。');
    if (found.status === 'running' || found.status === 'pending')
      throw new Error('调用尚未结束，请返回运行看板查看进度。');
    if (!disposed && version === requestVersion) {
      run.value = result;
      entry.value = found;
    }
  } catch (cause) {
    if (!disposed && version === requestVersion)
      error.value = cause instanceof Error ? cause.message : '输出详情读取失败';
  } finally {
    if (!disposed && version === requestVersion) loading.value = false;
  }
}
function goBack() {
  if (fromHistory.value) {
    void router.push({
      path: '/iq-test/history',
      query: typeof route.query.runId === 'string' ? { runId: route.query.runId } : {},
    });
  } else void router.push('/iq-test/task');
}
async function openRawFile() {
  if (!entry.value?.hasRawStream || opening.value) return;
  opening.value = true;
  try {
    await store.openRawFile(entry.value.id);
  } catch {
    /* 写入/目录反馈由统一 API 处理。 */
  } finally {
    opening.value = false;
  }
}
watch(
  () => [route.params.id, route.query.runId],
  () => void load(),
  { immediate: true },
);
onScopeDispose(() => {
  disposed = true;
  requestVersion += 1;
});
</script>

<template>
  <div class="h-full overflow-y-auto p-5">
    <div class="flex flex-wrap items-center gap-3">
      <a-button
        type="text"
        shape="circle"
        aria-label="返回来源页面"
        :title="`返回${sourceName}`"
        @click="goBack"
      >
        <template #icon><AppIcon name="ArrowLeftOutlined" /></template>
      </a-button>
      <div class="min-w-0 flex-1">
        <h2 class="m-0 flex flex-wrap items-center gap-2 text-lg font-medium">
          输出回放<span v-if="participant"> · {{ participant.modelName }}</span>
          <a-tag v-if="entry" :color="iqStatusColors[entry.status]">{{
            iqStatusLabels[entry.status]
          }}</a-tag>
        </h2>
        <p class="mb-0 mt-1 text-xs text-rp-secondary">来自{{ sourceName }}</p>
      </div>
      <a-tooltip
        :title="entry?.hasRawStream ? '打开末次实际尝试的原始流文件' : '未发起调用，无原始流文件'"
      >
        <span>
          <a-button :disabled="!entry?.hasRawStream" :loading="opening" @click="openRawFile">
            <template #icon><AppIcon name="FolderOpenOutlined" /></template>
            打开原始流文件
          </a-button>
        </span>
      </a-tooltip>
    </div>

    <div v-if="loading" class="mt-4 flex min-h-48 items-center justify-center"><a-spin /></div>
    <a-alert
      v-else-if="error"
      type="error"
      title="无法读取输出详情"
      :description="error"
      show-icon
      class="mt-4"
    >
      <template #action><a-button size="small" @click="load">重新加载</a-button></template>
    </a-alert>
    <template v-else-if="entry && participant && question">
      <p class="mt-3 mb-0 whitespace-pre-wrap wrap-break-word text-sm text-rp-secondary">
        题 {{ entry.questionIndex + 1 }} · {{ question.content }}
      </p>
      <section aria-label="实际使用的参与者快照" class="mt-4 min-w-0">
        <a-descriptions
          bordered
          size="small"
          :column="{ xs: 1, sm: 2 }"
          :items="snapshotItems"
          :styles="{
            label: { whiteSpace: 'nowrap' },
            content: { minWidth: 0, overflowWrap: 'anywhere' },
          }"
        >
          <template #title>
            <span class="inline-flex items-center gap-2">
              执行快照
              <a-tooltip title="此处展示本次调用实际使用的脱敏快照。" :trigger="['hover', 'focus']">
                <span
                  tabindex="0"
                  aria-label="执行快照说明"
                  class="inline-flex cursor-help text-sm font-normal text-rp-tertiary"
                >
                  <AppIcon name="QuestionCircleOutlined" />
                </span>
              </a-tooltip>
            </span>
          </template>
          <template #contentRender="{ item }">
            <a-popover
              v-if="item.key === 'parameters'"
              title="IQ 请求参数"
              trigger="click"
              placement="bottomLeft"
              :styles="{ root: { width: 'min(640px, calc(100vw - 32px))' } }"
            >
              <template #content>
                <pre
                  aria-label="完整 IQ 请求参数"
                  class="m-0 max-h-[min(60vh,480px)] overflow-auto whitespace-pre-wrap break-all font-mono text-xs"
                  >{{ parameterJson }}</pre>
              </template>
              <a-button
                color="primary"
                variant="link"
                size="small"
                aria-label="查看完整 IQ 请求参数"
              >
                查看全部参数
              </a-button>
            </a-popover>
            <span v-else :class="{ 'font-mono text-xs': item.key === 'endpoint' }">{{
              item.content
            }}</span>
          </template>
        </a-descriptions>
      </section>
      <a-alert
        type="warning"
        title="回放内容形态待定"
        description="先保留页面骨架；正文、参考答案、流分段、媒体和尝试切换将在首轮真实流数据取证后确定。"
        show-icon
        class="mt-5"
      />
      <div class="mt-3 rounded-lg border border-dashed border-rp-line bg-rp-subtle py-12">
        <a-empty description="回放正文留空 — 待真实流数据取证后补全" />
      </div>
    </template>
  </div>
</template>
