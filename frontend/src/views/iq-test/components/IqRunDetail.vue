<script setup lang="ts">
import type { IqEntry, IqRun } from '@/types/domain/iq-test';

import { onMounted, onScopeDispose, ref } from 'vue';
import { useRouter } from 'vue-router';

import {
  formatIqDuration,
  formatIqStrength,
  formatIqTime,
  iqStatusColors,
  iqStatusLabels,
  isIqEntryTerminal,
} from '../iq-presentation';

import AppIcon from '@/components/AppIcon.vue';

const props = withDefaults(
  defineProps<{ run: IqRun; source: 'run' | 'history'; showMetadata?: boolean }>(),
  { showMetadata: true },
);
const router = useRouter();
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  timer = setInterval(() => (now.value = Date.now()), 1000);
});
onScopeDispose(() => clearInterval(timer));

function entriesForQuestion(index: number) {
  return props.run.entries.filter((entry) => entry.questionIndex === index);
}
function elapsed(entry: IqEntry) {
  if (entry.status === 'running' && entry.startedAtMs != null)
    return formatIqDuration(Math.max(0, now.value - entry.startedAtMs));
  return formatIqDuration(entry.totalTimeMs);
}
function viewOutput(entry: IqEntry) {
  void router.push({
    path: `/iq-test/detail/${encodeURIComponent(entry.id)}`,
    query: { from: props.source, runId: props.run.id },
  });
}
</script>

<template>
  <div aria-label="逐题运行详情" class="space-y-4">
    <dl
      v-if="showMetadata"
      class="grid grid-cols-[auto_1fr] gap-x-5 gap-y-3 rounded-lg border border-rp-line bg-rp-subtle p-4 text-sm"
    >
      <dt class="text-rp-secondary">发起时间</dt>
      <dd class="m-0 tabular-nums">{{ formatIqTime(run.startedAtMs) }}</dd>
      <dt class="text-rp-secondary">调用参数</dt>
      <dd class="m-0">超时 {{ run.timeoutSeconds }} 秒 · 超时重试 {{ run.timeoutRetryMax }} 次</dd>
      <dt class="text-rp-secondary">参与者快照</dt>
      <dd class="m-0 flex min-w-0 flex-wrap gap-1.5">
        <a-tooltip
          v-for="participant in run.participants"
          :key="participant.snapshotId"
          :title="participant.siteName"
        >
          <a-tag class="max-w-full truncate">
            {{ participant.modelName }} · {{ formatIqStrength(participant) }}
          </a-tag>
        </a-tooltip>
      </dd>
    </dl>

    <section
      v-for="(question, questionIndex) in run.questions"
      :key="question.id"
      :aria-label="`题 ${questionIndex + 1}`"
      class="overflow-hidden rounded-lg border border-rp-line bg-rp-surface"
    >
      <div class="flex items-center gap-3 border-b border-rp-line bg-rp-subtle px-4 py-3">
        <h3 class="m-0 line-clamp-2 flex-1 text-sm font-medium" :title="question.content">
          题 {{ questionIndex + 1 }} · {{ question.content }}
        </h3>
        <a-tag
          v-if="run.summary.questionProgress[questionIndex]"
          :color="iqStatusColors[run.summary.questionProgress[questionIndex]!.status]"
          class="shrink-0"
        >
          {{ iqStatusLabels[run.summary.questionProgress[questionIndex]!.status] }} ·
          {{ run.summary.questionProgress[questionIndex]!.terminalEntries }} /
          {{ run.summary.questionProgress[questionIndex]!.totalEntries }}
        </a-tag>
      </div>
      <div class="space-y-2 p-3">
        <article
          v-for="entry in entriesForQuestion(questionIndex)"
          :key="entry.id"
          :aria-label="`${run.participants[entry.participantIndex]?.modelName} 的答题状态`"
          class="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md border border-rp-line px-3 py-3"
        >
          <div class="min-w-0 flex-1 basis-40">
            <div class="truncate font-medium">
              {{ run.participants[entry.participantIndex]?.modelName }}
            </div>
            <div class="mt-1 flex flex-wrap items-center gap-1 text-xs text-rp-secondary">
              <span>{{ run.participants[entry.participantIndex]?.siteName }}</span>
              <a-tag>{{ formatIqStrength(run.participants[entry.participantIndex]) }}</a-tag>
            </div>
          </div>
          <a-tag :color="iqStatusColors[entry.status]">
            <template v-if="entry.status === 'running'" #icon>
              <AppIcon name="LoadingOutlined" class="animate-spin" />
            </template>
            {{ iqStatusLabels[entry.status] }}
          </a-tag>
          <div class="min-w-0 basis-40 text-xs text-rp-secondary tabular-nums">
            <div
              v-if="entry.errorCode || entry.errorMessage"
              class="wrap-break-word text-(--ant-color-error)"
            >
              {{ entry.errorCode }} {{ entry.errorMessage }}
            </div>
            <template v-else-if="entry.status === 'running'">
              已用时 {{ elapsed(entry) }}
              <span v-if="entry.attempt > 1"> · 第 {{ entry.attempt }} 次尝试</span>
            </template>
            <template v-else-if="entry.status === 'pending'">等待前一题完成</template>
            <template v-else>
              <div>
                首 Token {{ formatIqDuration(entry.firstTokenMs) }} · 总耗时 {{ elapsed(entry) }}
              </div>
              <div v-if="entry.outputTokens != null" class="mt-1">
                输出 {{ entry.outputTokens.toLocaleString() }} tokens
              </div>
              <div v-else-if="entry.attempt === 0" class="mt-1">未发起调用</div>
            </template>
          </div>
          <a-tooltip
            v-if="isIqEntryTerminal(entry.status)"
            :title="entry.status === 'completed' ? '查看输出' : '查看详情'"
          >
            <a-button
              color="primary"
              variant="text"
              shape="circle"
              :aria-label="`${entry.status === 'completed' ? '查看输出' : '查看详情'} ${entry.id}`"
              @click="viewOutput(entry)"
            >
              <template #icon><AppIcon name="FileSearchOutlined" /></template>
            </a-button>
          </a-tooltip>
        </article>
      </div>
    </section>
  </div>
</template>
