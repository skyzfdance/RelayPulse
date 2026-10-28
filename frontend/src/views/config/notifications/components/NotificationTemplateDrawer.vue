<script setup lang="ts">
import type { NotificationTemplateDrawerProps } from './use-notification-template-drawer';
import type { NotificationTemplateMode } from '@/types/domain/notifications';

import { graphemeCount } from '../template-logic';
import { useNotificationTemplateDrawer } from './use-notification-template-drawer';

import AppIcon from '@/components/AppIcon.vue';

const props = defineProps<NotificationTemplateDrawerProps>();

const emit = defineEmits<{
  close: [];
  saved: [];
}>();

const {
  form,
  loading,
  custom,
  markerOptions,
  variableMenu,
  validation,
  invalid,
  preview,
  testError,
  restoreDefault,
  changeTemplateType,
  close,
  rememberRange,
  testTemplate,
  save,
} = useNotificationTemplateDrawer(props, {
  onClose: () => emit('close'),
  onSaved: () => emit('saved'),
});
</script>

<template>
  <a-drawer
    :open="open"
    size="large"
    :loading="loading && !form"
    :mask-closable="false"
    :closable="{ disabled: loading }"
    :keyboard="!loading"
    @close="close"
  >
    <template #title>
      <div>
        <strong>编辑通知模板</strong>
        <div v-if="form" class="mt-0.5 text-xs font-normal text-rp-tertiary">
          {{ form.title }} · {{ form.channel }}
        </div>
      </div>
    </template>

    <template #extra>
      <a-space>
        <a-button
          :disabled="loading || !testAvailable || invalid || !!testError || !form"
          @click="testTemplate"
        >
          <template #icon><AppIcon name="SendOutlined" /></template>
          测试当前模板
        </a-button>
        <a-button :disabled="loading" @click="close">取消</a-button>
        <a-button type="primary" :disabled="invalid || !form" :loading="loading" @click="save">
          保存
        </a-button>
      </a-space>
    </template>

    <div v-if="form" class="flex flex-col gap-5">
      <div class="flex items-center gap-2">
        <a-segmented
          :value="form.templateType"
          :disabled="loading"
          :options="[
            { label: '默认模板', value: 'default' },
            { label: '自定义模板', value: 'custom' },
          ]"
          block
          class="flex-1"
          @change="changeTemplateType($event as NotificationTemplateMode)"
        />
        <a-button :disabled="loading" title="恢复默认" @click="restoreDefault">
          <template #icon><AppIcon name="ReloadOutlined" /></template>
        </a-button>
      </div>

      <a-form layout="vertical" :required-mark="false">
        <a-form-item label="标题标记">
          <a-select
            v-model:value="form.markerId"
            :options="markerOptions"
            :disabled="loading || !custom"
          />
        </a-form-item>

        <a-form-item
          :validate-status="validation.title ? 'error' : undefined"
          :help="validation.title"
        >
          <template #label>
            <span class="flex w-full items-center justify-between gap-4">
              <span>标题</span>
              <span class="text-xs font-normal text-rp-tertiary">
                {{ graphemeCount(form.titleTemplate) }} / 120
              </span>
            </span>
          </template>
          <a-input v-model:value="form.titleTemplate" :disabled="loading || !custom" />
        </a-form-item>

        <a-form-item
          :validate-status="validation.body ? 'error' : undefined"
          :help="validation.body"
        >
          <template #label>
            <span class="flex w-full items-center justify-between gap-4">
              <span>通知详情</span>
              <span class="text-xs font-normal text-rp-tertiary">
                {{ graphemeCount(form.bodyTemplate) }} / 500
              </span>
            </span>
          </template>
          <a-textarea
            v-model:value="form.bodyTemplate"
            :disabled="loading || !custom"
            :auto-size="{ minRows: 6, maxRows: 10 }"
            @click="rememberRange"
            @keyup="rememberRange"
            @select="rememberRange"
          />
          <div class="mt-2 flex items-center justify-between">
            <span class="text-xs text-rp-tertiary">在光标位置插入变量</span>
            <a-dropdown :menu="variableMenu" :trigger="['click']" :disabled="loading || !custom">
              <a-button type="text" size="small" :disabled="loading || !custom">
                <template #icon><AppIcon name="CodeOutlined" /></template>
                插入变量
                <AppIcon name="DownOutlined" />
              </a-button>
            </a-dropdown>
          </div>
        </a-form-item>
      </a-form>

      <section>
        <div class="mb-2 flex items-center justify-between gap-4">
          <h3 class="text-sm font-medium">通知预览</h3>
          <span class="text-xs text-rp-tertiary">固定示例数据</span>
        </div>
        <div class="rounded-ant-lg border border-rp-line bg-rp-subtle p-4">
          <div class="font-semibold text-rp">{{ preview.title || '—' }}</div>
          <div class="mt-1.5 text-sm leading-6 whitespace-pre-wrap text-rp-secondary">
            {{ preview.body || '—' }}
          </div>
        </div>
        <a-alert v-if="testError" class="mt-3" type="warning" :title="testError" show-icon />
      </section>
    </div>
  </a-drawer>
</template>
