<script setup lang="ts">
import type { ThemeMode } from '@/types/theme';

import { computed } from 'vue';

import { OptionCard, OptionCardGrid } from '@/components/option-card';
import { formatBytes } from '@/utils/format-bytes';

import { useGeneral } from './use-general';

import AppIcon from '@/components/AppIcon.vue';

const general = useGeneral();
const { settings, logStats } = general;

const THEME_OPTIONS = [
  { value: 'system', label: '跟随系统' },
  { value: 'light', label: '浅色' },
  { value: 'dark', label: '深色' },
];

const logDescription = computed(() =>
  logStats.value
    ? `${logStats.value.fileCount} 个文件 · ${formatBytes(logStats.value.totalBytes)}`
    : '正在读取日志大小',
);
</script>

<template>
  <div v-if="settings" class="h-full overflow-y-auto p-5">
    <section class="pb-3">
      <h2 class="mb-1 text-base font-medium">启动与后台</h2>
      <OptionCardGrid>
        <OptionCard
          title="开机自动启动"
          description="登录系统后自动在状态栏运行 RelayPulse"
          control-align="end"
        >
          <a-switch
            :checked="settings.launchAtStartup"
            @change="general.setSetting('launchAtStartup', $event as boolean)"
          />
        </OptionCard>
        <OptionCard
          title="关闭主窗口后继续运行"
          description="关闭配置窗口后继续执行后台探测"
          control-align="end"
        >
          <a-switch
            :checked="settings.continueInBackground"
            @change="general.setSetting('continueInBackground', $event as boolean)"
          />
        </OptionCard>

        <OptionCard
          title="显示状态栏图标"
          description="点击图标查看状态面板,关闭后,后台探测不受影响"
          control-align="end"
        >
          <a-switch
            :checked="settings.showTrayIcon"
            @change="general.setSetting('showTrayIcon', $event as boolean)"
          />
        </OptionCard>
      </OptionCardGrid>
    </section>

    <section class="pb-3">
      <h2 class="mb-1 text-base font-medium">外观与语言</h2>
      <OptionCardGrid>
        <OptionCard title="主题" description="跟随系统时会自动切换浅色和深色模式">
          <a-segmented
            :value="settings.theme"
            :options="THEME_OPTIONS"
            @change="general.setSetting('theme', $event as ThemeMode)"
          />
        </OptionCard>
        <OptionCard title="界面语言" description="跟随系统时会自动切换浅色和深色模式">
          <a-select
            :value="settings.language"
            class="w-42.5"
            :options="[{ value: 'zh-CN', label: '简体中文' }]"
            @change="general.setSetting('language', $event as 'zh-CN')"
          />
        </OptionCard>
      </OptionCardGrid>
    </section>

    <section>
      <h2 class="mb-1 text-base font-medium">本地数据</h2>
      <OptionCardGrid>
        <OptionCard title="探测记录保留时间" description="过期记录会自动清理，不影响站点和模型配置">
          <a-select
            :value="settings.historyRetentionDays"
            class="w-42.5"
            :options="[{ value: 30, label: '保留 30 天' }]"
            @change="general.setSetting('historyRetentionDays', $event as number)"
          />
        </OptionCard>
        <OptionCard title="诊断日志" :description="logDescription">
          <div class="flex gap-2">
            <a-button @click="general.openLogs()">
              <template #icon><AppIcon name="FolderOpenOutlined" /></template>
              打开日志目录
            </a-button>
            <a-popconfirm
              title="清理本地诊断日志？"
              description="只会删除日志文件，不会删除探测记录或站点配置。"
              ok-text="清理"
              cancel-text="取消"
              ok-type="danger"
              @confirm="general.clearLogs()"
            >
              <a-button danger aria-label="清理日志">
                <template #icon><AppIcon name="DeleteOutlined" /></template>
              </a-button>
            </a-popconfirm>
          </div>
        </OptionCard>
        <OptionCard
          title="诊断包"
          description="包含错误链路、失败探测摘要和日志容量信息，不包含请求响应正文"
        >
          <a-button @click="general.exportPackage()">
            <template #icon><AppIcon name="ExportOutlined" /></template>
            导出诊断包
          </a-button>
        </OptionCard>
      </OptionCardGrid>
    </section>
  </div>
</template>
