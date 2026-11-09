<script setup lang="ts">
import type { IqParticipant } from '@/types/domain/iq-test';
import type { TableProps } from 'antdv-next';

import { computed, onMounted, onScopeDispose, ref, watch } from 'vue';

import { useIqTestStore } from '@/stores/iq-test';
import { iqStartError } from '@/views/iq-test/iq-parameters';

import AppIcon from '@/components/AppIcon.vue';
import IqParticipantDrawer from '@/views/iq-test/components/IqParticipantDrawer.vue';
import IqRunDetail from '@/views/iq-test/components/IqRunDetail.vue';
import IqQuestionSelector from '@/views/iq-test/task/IqQuestionSelector.vue';

const store = useIqTestStore();
const selected = ref<string[]>([]);
const timeout = ref<number | null>(120);
const retries = ref<number | null>(5);
const drawer = ref(false);
const starting = ref(false);
const aborting = ref(false);
const startError = computed(() =>
  iqStartError(selected.value, store.participants, timeout.value, retries.value),
);
const columns: TableProps<IqParticipant>['columns'] = [
  { title: '模型 / 站点', key: 'model' },
  { title: '推理强度', dataIndex: 'preset', key: 'preset', width: 130 },
  { title: '预检', key: 'precheck', width: 100 },
  { title: '', key: 'action', width: 72 },
];
const precheckLabels: Record<string, string> = {
  checking: '检测中',
  available: '可用',
  unavailable: '不可用',
};
const precheckColors: Record<string, string> = {
  checking: 'processing',
  available: 'success',
  unavailable: 'error',
};
watch(
  () => store.questions.filter((question) => question.enabled).map((question) => question.id),
  (ids) => {
    const available = new Set(ids);
    selected.value = selected.value.filter((id) => available.has(id));
  },
);
let disconnect: (() => void) | undefined;
let disposed = false;
onMounted(async () => {
  disconnect = await store.connect();
  if (disposed) disconnect();
  else await store.refresh();
});
onScopeDispose(() => {
  disposed = true;
  disconnect?.();
});
async function start() {
  if (startError.value || starting.value) return;
  starting.value = true;
  try {
    await store.start({
      questionIds: [...selected.value],
      timeoutSeconds: timeout.value!,
      timeoutRetryMax: retries.value!,
    });
  } catch {
    /* API 提供错误反馈。 */
  } finally {
    starting.value = false;
  }
}
async function abort() {
  aborting.value = true;
  try {
    await store.abort();
  } catch {
    /* API 提供错误反馈。 */
  } finally {
    aborting.value = false;
  }
}
async function recheck(id: string) {
  try {
    await store.recheckParticipant(id);
  } catch {
    /* 保留失败状态与 API 提示。 */
  }
}
async function remove(id: string) {
  try {
    await store.removeParticipant(id);
  } catch {
    /* API 提供错误反馈。 */
  }
}
</script>

<template>
  <div class="p-5" data-testid="iq-task">
    <a-alert v-if="store.error" type="error" :title="store.error" show-icon class="mb-4">
      <template #action><a-button size="small" @click="store.refresh()">重试</a-button></template>
    </a-alert>
    <div v-if="store.loading && !store.questions.length" class="grid min-h-60 place-items-center">
      <a-spin />
    </div>
    <template v-else-if="store.activeRun">
      <section
        class="mb-4 rounded-lg border border-rp-line bg-rp-surface p-4"
        aria-label="运行进度"
      >
        <div class="mb-2 flex flex-wrap items-center justify-between gap-3">
          <div>
            <strong>测试进行中</strong
            ><span class="ml-3 text-xs text-rp-secondary"
              >题目 {{ store.activeRun.summary.currentQuestion }} /
              {{ store.activeRun.summary.totalQuestions }} · 已用
              {{ Math.floor(store.activeRun.summary.durationMs / 1000) }} 秒</span
            >
          </div>
          <a-popconfirm
            title="中止本次测试？"
            description="已完成结果保留，正在运行及尚未开始的调用将标记为已中止。"
            ok-text="中止测试"
            cancel-text="继续测试"
            ok-type="danger"
            @confirm="abort"
          >
            <a-button danger :loading="aborting">中止测试</a-button>
          </a-popconfirm>
        </div>
        <a-progress :percent="store.activeRun.summary.progressPercent" status="active" />
        <p class="m-0! text-xs text-rp-tertiary">
          已结束 {{ store.activeRun.summary.terminalEntries }} /
          {{ store.activeRun.summary.totalEntries }} 次调用 · 成功
          {{ store.activeRun.summary.successfulEntries }} 次
        </p>
      </section>
      <IqRunDetail :run="store.activeRun" source="run" />
    </template>
    <template v-else>
      <div class="iq-config-grid mb-4 grid gap-4">
        <IqQuestionSelector
          v-model:selected-ids="selected"
          :questions="store.questions"
          :loading="store.loading"
        />
        <a-card title="测试参与者" size="small" class="min-w-0">
          <template #extra
            ><a-button size="small" type="primary" ghost @click="drawer = true"
              ><template #icon><AppIcon name="PlusOutlined" /></template>添加参与者</a-button
            ></template
          >
          <a-table
            :columns="columns"
            :data-source="store.participants"
            row-key="id"
            size="small"
            :pagination="false"
            :scroll="{ x: 430 }"
          >
            <template #bodyCell="{ column, record }">
              <template v-if="column.key === 'model'"
                ><div class="max-w-45 truncate font-medium" :title="record.modelName">
                  {{ record.modelName }}
                </div>
                <div class="text-xs text-rp-tertiary">{{ record.siteName }}</div></template
              >
              <template v-else-if="column.key === 'preset'"
                ><a-tag color="blue">{{ record.preset }}</a-tag></template
              >
              <template v-else-if="column.key === 'precheck'"
                ><a-tooltip :title="record.precheck.message || '当前执行快照预检'"
                  ><a-tag :color="precheckColors[record.precheck.status]">{{
                    precheckLabels[record.precheck.status]
                  }}</a-tag></a-tooltip
                ></template
              >
              <template v-else-if="column.key === 'action'"
                ><div class="flex">
                  <a-tooltip
                    v-if="record.precheck.status === 'unavailable'"
                    title="读取最新配置并重新探测"
                    ><a-button
                      type="text"
                      shape="circle"
                      size="small"
                      aria-label="重新探测"
                      class="text-rp-primary!"
                      @click="recheck(record.id)"
                      ><template #icon><AppIcon name="ReloadOutlined" /></template></a-button
                  ></a-tooltip>
                  <a-tooltip title="移除参与者"
                    ><a-button
                      type="text"
                      shape="circle"
                      size="small"
                      danger
                      aria-label="移除参与者"
                      @click="remove(record.id)"
                      ><template #icon><AppIcon name="DeleteOutlined" /></template></a-button
                  ></a-tooltip></div
              ></template>
            </template>
            <template #emptyText><a-empty description="添加模型与推理强度组合" /></template>
          </a-table>
          <p class="mt-3! mb-0! text-xs text-rp-tertiary">
            预检使用各自快照；同一模型可添加多个强度。
          </p>
        </a-card>
      </div>
      <a-card title="调用参数" size="small">
        <div class="flex flex-wrap gap-x-8 gap-y-3">
          <label class="flex items-center gap-2"
            ><span>超时秒数</span
            ><a-input-number
              v-model:value="timeout"
              aria-label="超时秒数"
              :min="60"
              :max="300"
              :status="
                timeout === null || !Number.isInteger(timeout) || timeout < 60 || timeout > 300
                  ? 'error'
                  : undefined
              "
            /><span class="text-xs text-rp-tertiary">连续无有效输出</span></label
          >
          <label class="flex items-center gap-2"
            ><span>超时重试</span
            ><a-input-number
              v-model:value="retries"
              aria-label="超时重试"
              :min="5"
              :max="20"
              :status="
                retries === null || !Number.isInteger(retries) || retries < 5 || retries > 20
                  ? 'error'
                  : undefined
              "
            /><span class="text-xs text-rp-tertiary">次，不含首次，仅超时触发</span></label
          >
        </div>
      </a-card>
      <div class="mt-6">
        <div class="flex flex-wrap items-center gap-3">
          <a-tooltip
            title="请选择题目、添加参与者并确保每份快照预检可用；超时须为60–300秒整数，重试须为5–20次整数。"
            ><span
              ><a-button
                type="primary"
                :disabled="Boolean(startError) || Boolean(store.error)"
                :loading="starting"
                @click="start"
                ><template #icon><AppIcon name="PlayCircleOutlined" /></template>开始测试</a-button
              ></span
            ></a-tooltip
          >
          <a-tag v-if="startError" color="error">{{ startError }}</a-tag>
        </div>
        <p class="mt-4! mb-0! text-xs text-rp-tertiary">
          {{ selected.length }} 题 × {{ store.participants.length }} 参与者 =
          {{ selected.length * store.participants.length }}
          次请求（不含重试）。正式测试将产生模型调用费用。
        </p>
      </div>
      <IqParticipantDrawer v-model:open="drawer" />
    </template>
  </div>
</template>

<style scoped>
.iq-config-grid {
  grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr);
}
@media (max-width: 1050px) {
  .iq-config-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
