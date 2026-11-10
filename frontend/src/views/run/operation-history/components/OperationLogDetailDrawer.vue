<script setup lang="ts">
import type { OperationLogDetail } from '@/types/domain/operation-logs';

import { computed } from 'vue';

import { formatRecordTime } from '../../records/records-presentation';
import { presentOperationContent } from '../operation-log-presentation';

const props = defineProps<{
  open: boolean;
  loading: boolean;
  error: string;
  missing: boolean;
  detail?: OperationLogDetail;
}>();
const emit = defineEmits<{ close: []; retry: [] }>();
const resultLabels = { success: '成功', failure: '失败', cancelled: '已取消', blocked: '受阻' };
const resultColors = {
  success: 'var(--ant-color-success)',
  failure: 'var(--ant-color-error)',
  cancelled: 'var(--ant-color-text-tertiary)',
  blocked: 'var(--ant-color-warning)',
};
const content = computed(() => {
  const detail = props.detail;
  return detail && presentOperationContent(detail.content, detail.item.action);
});
const basicInfo = computed(() => {
  const item = props.detail?.item;
  if (!item) return [];
  return [
    { label: '时间', value: formatRecordTime(item.occurredAt, true) },
    { label: '来源', value: item.source === 'system' ? '系统' : '用户' },
    { label: '执行者', value: item.actorSnapshot },
    { label: '操作对象', value: item.objectSnapshot },
  ];
});
const isLong = (value: string) => value.length > 240;
</script>

<template>
  <a-drawer
    :open="open"
    :size="600"
    :closable="{ 'aria-label': '关闭操作详情' }"
    :styles="{
      wrapper: { maxWidth: '100vw' },
      header: { padding: '12px 20px' },
      body: { padding: '16px 20px' },
    }"
    destroy-on-hidden
    @close="emit('close')"
  >
    <template #title>
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <span v-overflow-tooltip class="min-w-0 text-base font-semibold">{{
          detail?.item.actionLabel ?? '操作详情'
        }}</span>
        <a-tag
          v-if="detail"
          :color="resultColors[detail.item.result]"
          class="m-0! shrink-0"
          variant="outlined"
          >{{ resultLabels[detail.item.result] }}</a-tag
        >
      </div>
    </template>

    <section aria-label="操作详情内容" :aria-busy="loading">
      <div v-if="loading" role="status" aria-label="操作详情加载中">
        <a-skeleton active :paragraph="{ rows: 7 }" />
      </div>
      <a-empty v-else-if="missing" class="py-12">
        <template #description>这条操作记录已不存在或已过期</template>
      </a-empty>
      <a-alert
        v-else-if="error"
        type="error"
        show-icon
        title="操作详情读取失败"
        :description="error"
        role="alert"
      >
        <template #action>
          <a-button size="small" @click="emit('retry')">重新加载</a-button>
        </template>
      </a-alert>
      <template v-else-if="detail && content">
        <p class="mb-4! text-sm leading-6 text-rp-secondary">{{ detail.explanation }}</p>

        <section class="border-b border-rp-line py-4" aria-label="基本信息">
          <h3 class="mb-3! text-sm font-semibold">基本信息</h3>
          <dl class="m-0! grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-2">
            <div v-for="field in basicInfo" :key="field.label" class="min-w-0">
              <dt class="text-xs text-rp-tertiary">{{ field.label }}</dt>
              <dd class="m-0! mt-1! wrap-break-word text-sm">{{ field.value }}</dd>
            </div>
          </dl>
        </section>

        <section class="py-4" aria-label="本次事件内容">
          <h3 class="mb-3! text-sm font-semibold">本次事件</h3>
          <div
            v-if="content.kind === 'snapshot' || content.kind === 'presentation'"
            class="space-y-3"
          >
            <div v-for="field in content.fields" :key="field.label" class="min-w-0">
              <div class="mb-1 text-xs text-rp-tertiary">{{ field.label }}</div>
              <details v-if="!field.sensitive && isLong(field.value)" class="detail-expand">
                <summary>展开长内容</summary>
                <pre class="detail-value">{{ field.value }}</pre>
              </details>
              <pre v-else-if="!field.sensitive" class="detail-value">{{ field.value }}</pre>
              <div v-else class="text-sm">{{ field.value }}</div>
            </div>
          </div>
          <div v-else-if="content.kind === 'changes'" class="space-y-4">
            <div v-for="change in content.changes" :key="change.field" class="min-w-0">
              <div class="mb-2 text-xs font-medium text-rp-secondary">{{ change.field }}</div>
              <div class="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2">
                <div role="group" aria-label="修改前" class="min-w-0">
                  <details v-if="!change.sensitive && isLong(change.before)" class="detail-expand">
                    <summary>展开长内容</summary>
                    <pre class="detail-value">{{ change.before }}</pre>
                  </details>
                  <pre v-else class="detail-value">{{ change.before }}</pre>
                </div>
                <div role="group" aria-label="修改后" class="change-after min-w-0">
                  <details v-if="!change.sensitive && isLong(change.after)" class="detail-expand">
                    <summary>展开长内容</summary>
                    <pre class="detail-value">{{ change.after }}</pre>
                  </details>
                  <pre v-else class="detail-value">{{ change.after }}</pre>
                </div>
              </div>
            </div>
          </div>
          <template v-else-if="content.kind === 'deletion'">
            <p class="mb-3! text-sm text-rp-secondary">{{ content.summary }}</p>
            <dl class="m-0! grid grid-cols-1 gap-3 sm:grid-cols-2">
              <div v-for="field in content.fields" :key="field.label">
                <dt class="text-xs text-rp-tertiary">{{ field.label }}</dt>
                <dd class="m-0! mt-1! wrap-break-word text-sm">{{ field.value }}</dd>
              </div>
            </dl>
          </template>
          <div v-else-if="content.kind === 'notification'" class="space-y-3">
            <div>
              <div class="text-xs text-rp-tertiary">触发原因</div>
              <p class="m-0! mt-1! text-sm">{{ content.trigger }}</p>
            </div>
            <div>
              <div class="text-xs text-rp-tertiary">
                {{ detail.item.result === 'cancelled' ? '通知标题（未发送）' : '实际发送标题' }}
              </div>
              <pre class="detail-value mt-1!">{{ content.title }}</pre>
            </div>
            <div>
              <div class="text-xs text-rp-tertiary">
                {{ detail.item.result === 'cancelled' ? '通知正文（未发送）' : '实际发送正文' }}
              </div>
              <details v-if="isLong(content.body)" class="detail-expand mt-1!">
                <summary>展开完整正文</summary>
                <pre class="detail-value">{{ content.body }}</pre>
              </details>
              <pre v-else class="detail-value mt-1!">{{ content.body }}</pre>
            </div>
            <div>
              <div class="text-xs text-rp-tertiary">发送结果</div>
              <p class="m-0! mt-1! text-sm">{{ content.sendResult }}</p>
            </div>
          </div>
          <dl
            v-else-if="content.kind === 'cleanup'"
            class="m-0! grid grid-cols-1 gap-3 sm:grid-cols-2"
          >
            <div>
              <dt class="text-xs text-rp-tertiary">清理对象</dt>
              <dd class="m-0! mt-1! text-sm">{{ content.target }}</dd>
            </div>
            <div>
              <dt class="text-xs text-rp-tertiary">清理范围</dt>
              <dd class="m-0! mt-1! wrap-break-word text-sm">{{ content.scope }}</dd>
            </div>
            <div>
              <dt class="text-xs text-rp-tertiary">清理数量</dt>
              <dd class="m-0! mt-1! text-sm tabular-nums">{{ content.count }} 项</dd>
            </div>
            <div v-if="content.space">
              <dt class="text-xs text-rp-tertiary">空间变化</dt>
              <dd class="m-0! mt-1! text-sm">{{ content.space }}</dd>
            </div>
          </dl>
          <dl
            v-else-if="content.kind === 'export'"
            class="m-0! grid grid-cols-1 gap-3 sm:grid-cols-2"
          >
            <div>
              <dt class="text-xs text-rp-tertiary">文件</dt>
              <dd class="m-0! mt-1! break-all text-sm">{{ content.filename }}</dd>
            </div>
            <div>
              <dt class="text-xs text-rp-tertiary">文件大小</dt>
              <dd class="m-0! mt-1! text-sm">{{ content.fileSize }}</dd>
            </div>
            <div>
              <dt class="text-xs text-rp-tertiary">结果</dt>
              <dd class="m-0! mt-1! text-sm">{{ content.outcome }}</dd>
            </div>
          </dl>
          <a-alert
            v-else-if="content.kind === 'failure'"
            :type="detail.item.result === 'blocked' ? 'warning' : 'error'"
            show-icon
            :title="content.activity"
            :description="`${content.target}：${content.reason}`"
          />
          <a-alert
            v-else-if="content.kind === 'probe_failure'"
            type="error"
            show-icon
            title="未生成探测记录"
            :description="content.reason"
          />
        </section>
      </template>
    </section>
  </a-drawer>
</template>

<style scoped>
.detail-value {
  max-height: 240px;
  margin: 0;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  border-radius: 4px;
  background: var(--ant-color-fill-quaternary);
  padding: 10px 12px;
  color: var(--ant-color-text);
  font: inherit;
  font-size: 12px;
  line-height: 1.6;
}

.detail-expand > summary {
  margin: 4px 0;
  cursor: pointer;
  color: var(--ant-color-primary);
  font-size: 12px;
}

.change-after > .detail-value,
.change-after > .detail-expand {
  background: var(--ant-color-primary-active);
  color: var(--ant-color-text-light-solid);
}

.change-after > .detail-expand {
  border-radius: 4px;
  padding: 10px 12px;
}

.change-after > .detail-expand > summary {
  margin: 0;
  color: inherit;
}

.change-after > .detail-expand > .detail-value {
  margin-top: 8px;
  background: transparent;
  padding: 0;
  color: inherit;
}
</style>
