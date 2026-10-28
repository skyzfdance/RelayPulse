<script setup lang="ts">
import { OptionCard, OptionCardGrid } from '@/components/option-card';

import { PERMISSION } from './permission';
import { useNotifications } from './use-notifications';

import NotificationTemplateDrawer from './components/NotificationTemplateDrawer.vue';
import AppIcon from '@/components/AppIcon.vue';

const notifications = useNotifications();
const { settings, testAvailable, templateOpen, templateRuleId } = notifications;

const columns = [
  { key: 'event', title: '事件' },
  { key: 'channel', title: '通知渠道', width: 160 },
  { key: 'template', title: '模板', width: 120, align: 'center' as const },
  { key: 'status', title: '启用', width: 88, align: 'center' as const },
  { key: 'actions', title: '操作', width: 90, align: 'center' as const },
];
</script>

<template>
  <div v-if="settings" class="h-full overflow-y-auto p-5">
    <section class="pb-3">
      <h2 class="mb-1 text-base font-medium">通知服务</h2>
      <OptionCardGrid>
        <OptionCard
          title="启用消息通知"
          description="关闭后保留当前规则，但不再发送任何通知"
          control-align="end"
        >
          <a-switch
            :checked="settings.notificationsEnabled"
            @change="notifications.setNotificationsEnabled($event as boolean)"
          />
        </OptionCard>
        <OptionCard
          title="系统通知"
          description="通过当前操作系统显示 RelayPulse 通知"
          control-align="end"
        >
          <div class="flex items-center gap-3">
            <a-badge
              :status="PERMISSION[settings.authorization].status"
              :text="PERMISSION[settings.authorization].label"
            />
            <a-button
              v-if="PERMISSION[settings.authorization].action === 'request'"
              @click="notifications.requestPermission()"
            >
              请求授权
            </a-button>
            <a-button
              v-else-if="PERMISSION[settings.authorization].action === 'test'"
              :disabled="!settings.notificationsEnabled"
              @click="notifications.sendTest()"
            >
              <template #icon><AppIcon name="SendOutlined" /></template>
              发送测试通知
            </a-button>
            <a-button
              v-else-if="PERMISSION[settings.authorization].action === 'settings'"
              @click="notifications.openSettings()"
            >
              去系统设置
            </a-button>
          </div>
        </OptionCard>
      </OptionCardGrid>
    </section>

    <section>
      <h2 class="mb-1 text-base font-medium">通知规则</h2>
      <a-table
        :columns="columns"
        :data-source="settings.rules"
        :pagination="false"
        row-key="id"
        size="middle"
        bordered
      >
        <template #bodyCell="{ column, record }">
          <div v-if="column.key === 'event'">
            <div class="font-medium">{{ record.title }}</div>
            <div class="mt-0.5 text-xs text-rp-tertiary">{{ record.subTitle }}</div>
          </div>
          <span v-else-if="column.key === 'channel'" class="text-rp-secondary">
            {{ record.channel }}
          </span>
          <span
            v-else-if="column.key === 'template'"
            class="text-xs"
            :class="record.templateType === 'custom' ? 'text-rp-primary' : 'text-rp-tertiary'"
          >
            {{ record.templateType === 'custom' ? '自定义' : '默认' }}
          </span>
          <a-switch
            v-else-if="column.key === 'status'"
            :checked="record.status"
            size="small"
            @change="notifications.setRuleStatus(record.id, $event as boolean)"
          />

          <span
            v-else-if="column.key === 'actions'"
            title="编辑模板"
            class="inline-flex cursor-pointer text-rp-secondary transition-colors hover:text-rp-primary"
            @click="notifications.openTemplate(record.id)"
          >
            <AppIcon name="EditOutlined" />
          </span>
        </template>
      </a-table>
    </section>
  </div>

  <NotificationTemplateDrawer
    :open="templateOpen"
    :rule-id="templateRuleId"
    :test-available="testAvailable"
    @close="notifications.closeTemplate()"
    @saved="notifications.load()"
  />
</template>
