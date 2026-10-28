<script setup lang="ts">
import type { ProbeRecordResponseContent } from '@/types/domain/records';

import { formatBytes } from '@/utils/format-bytes';

import { formatCount, formatDuration } from '../records-presentation';

defineProps<{
  content?: ProbeRecordResponseContent;
  loading: boolean;
  error: string;
  retryable: boolean;
  openingDirectory: boolean;
}>();
const emit = defineEmits<{ reload: []; openLogDirectory: [] }>();
</script>

<template>
  <section aria-label="原始返回内容" :aria-busy="loading">
    <div v-if="loading" role="status" aria-label="原始返回加载中">
      <a-skeleton active :title="false" :paragraph="{ rows: 4 }" />
    </div>
    <a-alert
      v-else-if="error"
      type="error"
      show-icon
      title="原始返回加载失败"
      :description="error"
      role="alert"
    >
      <template v-if="retryable" #action>
        <a-button size="small" @click="emit('reload')">重新加载</a-button>
      </template>
    </a-alert>
    <template v-else-if="content">
      <p v-if="content.state !== 'received'" class="m-0! text-xs text-rp-secondary">
        {{ content.state === 'not_received' ? '本次请求未收到响应' : '未保存响应信息' }}
      </p>
      <template v-else>
        <h4 class="m-0! mb-2! text-xs font-medium text-rp-secondary">响应头</h4>
        <p v-if="content.headers === null" class="m-0! text-xs text-rp-tertiary">未保存响应头</p>
        <p v-else-if="!content.headers.length" class="m-0! text-xs text-rp-tertiary">无响应头</p>
        <dl v-else aria-label="已脱敏响应头" class="m-0!">
          <div
            v-for="(header, index) in content.headers"
            :key="index"
            class="grid grid-cols-[112px_minmax(0,1fr)] gap-x-3 border-b border-rp-line py-2 last:border-0 max-[480px]:grid-cols-1 max-[480px]:gap-y-1"
          >
            <dt class="text-xs leading-5 wrap-anywhere text-rp-secondary">{{ header.name }}</dt>
            <dd class="m-0! font-mono text-xs leading-5 wrap-anywhere">{{ header.value }}</dd>
          </div>
        </dl>
        <section
          v-if="content.kind === 'stream' && content.chunkCount !== null && content.chunks !== null"
          class="pt-4"
        >
          <h4 class="m-0! mb-2! text-xs font-medium text-rp-secondary">
            接收数据块（{{ formatCount(content.chunkCount) }}）
          </h4>
          <ul
            aria-label="接收数据块"
            class="m-0! grid max-h-60 list-none grid-cols-2 gap-2 overflow-auto p-0 max-[480px]:grid-cols-1"
          >
            <li
              v-for="chunk in content.chunks"
              :key="chunk.index"
              class="rounded-md border border-rp-line bg-rp-subtle px-2 py-1.5 text-xs leading-5 tabular-nums"
            >
              <div class="flex flex-wrap items-center gap-x-2">
                <span>数据块 {{ formatCount(chunk.index) }}</span>
                <span class="text-rp-secondary">
                  · {{ chunk.bytes === null ? '—' : formatBytes(chunk.bytes) }}
                </span>
              </div>
              <div class="text-rp-secondary">
                {{
                  chunk.elapsedMs === null
                    ? '接收时间未记录'
                    : `请求开始后 ${formatDuration(chunk.elapsedMs)} 收到`
                }}
              </div>
            </li>
          </ul>
          <p v-if="!content.chunks.length" class="m-0! text-xs text-rp-tertiary">
            无接收数据块记录
          </p>
        </section>
        <section v-if="content.errorResponse" aria-label="错误响应体" class="pt-4">
          <h4 class="m-0! mb-2! text-xs font-medium text-rp-secondary">原始响应体</h4>
          <p
            v-if="content.errorResponse.bodyComplete === false"
            class="m-0! mb-2! text-xs text-rp-secondary"
          >
            响应未接收完整，以下为已保存内容。
          </p>
          <p v-if="content.errorResponse.body === null" class="m-0! text-xs text-rp-tertiary">
            未保存可展示的响应体，请通过日志查看。
          </p>
          <p v-else-if="content.errorResponse.body === ''" class="m-0! text-xs text-rp-tertiary">
            响应体为空
          </p>
          <pre
            v-else
            aria-label="原始响应体"
            tabindex="0"
            class="m-0! max-h-64 overflow-auto rounded-md bg-rp-subtle p-3 font-mono text-xs! leading-5! whitespace-pre-wrap wrap-anywhere"
          ><code>{{ content.errorResponse.body }}</code></pre>
        </section>
      </template>
      <section aria-label="响应日志" class="pt-4">
        <h4 class="m-0! mb-2! text-xs font-medium text-rp-secondary">日志路径</h4>
        <p class="m-0! font-mono text-xs leading-5 wrap-anywhere">
          {{ content.logFile?.path ?? '未保存日志路径' }}
        </p>
        <div v-if="content.logFile" class="mt-2 flex flex-wrap gap-2">
          <a-button
            size="small"
            aria-label="打开探测日志目录"
            :loading="openingDirectory"
            :disabled="openingDirectory || !content.logFile.canOpenDirectory"
            @click="emit('openLogDirectory')"
          >
            打开目录
          </a-button>
        </div>
      </section>
    </template>
  </section>
</template>
