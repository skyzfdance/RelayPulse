<script setup lang="ts">
import type { PanelSiteGroup, PanelSnapshot } from '@/types/domain/panel';

import { computed } from 'vue';

import {
  formatPanelDuration,
  formatPanelTime,
  panelModelLabels,
  panelTimeTitle,
} from '../panel-presentation';

import AppIcon from '@/components/AppIcon.vue';

const props = defineProps<{
  snapshot: PanelSnapshot;
  refreshing?: boolean;
  frosted?: boolean;
}>();

// 仅向直接宿主暴露动作意图，不广播业务事件，不决定窗口或路由行为。
const emit = defineEmits<{
  refresh: [];
  openSettings: [];
  viewAll: [];
}>();

const probing = computed(() => Boolean(props.refreshing) || props.snapshot.activeProbe !== null);
const maxVisibleModels = 3;
const visibleSites = computed(() => {
  let remaining = maxVisibleModels;
  const sites: PanelSiteGroup[] = [];

  for (const site of props.snapshot.sites) {
    if (remaining === 0) break;
    const models = site.models.slice(0, remaining);
    if (models.length === 0) continue;
    sites.push({ ...site, models });
    remaining -= models.length;
  }

  return sites;
});
const visibleModelCount = computed(() =>
  visibleSites.value.reduce((count, site) => count + site.models.length, 0),
);
</script>

<template>
  <article
    class="panel-surface"
    :data-state="snapshot.state"
    :data-frosted="frosted === true"
    aria-label="RelayPulse 小面板"
  >
    <header class="panel-header">
      <div class="panel-brand">
        <span class="panel-logo" aria-hidden="true"><AppIcon name="ThunderboltFilled" /></span>
        <span class="panel-name">RelayPulse</span>
      </div>
      <div class="panel-actions">
        <a-tooltip :title="probing ? '正在探测' : '立即探测'" placement="bottom">
          <a-button
            type="text"
            size="small"
            class="panel-icon-button"
            aria-label="立即探测"
            :loading="probing"
            :disabled="probing"
            @click="emit('refresh')"
          >
            <template #icon><AppIcon name="ReloadOutlined" /></template>
          </a-button>
        </a-tooltip>
        <a-tooltip title="设置" placement="bottom">
          <a-button
            type="text"
            size="small"
            class="panel-icon-button"
            aria-label="打开设置"
            @click="emit('openSettings')"
          >
            <template #icon><AppIcon name="SettingOutlined" /></template>
          </a-button>
        </a-tooltip>
      </div>
    </header>

    <section v-if="snapshot.state === 'waiting'" class="panel-summary">
      <div class="panel-status-mark" aria-hidden="true">
        <AppIcon name="ClockCircleOutlined" />
      </div>
      <div class="panel-summary-copy">
        <h1 class="panel-status-title">等待探测</h1>
        <p class="panel-waiting-hint">第一轮结果将在这里呈现</p>
        <p class="panel-counts">
          <span>{{ snapshot.siteCount }} 个站点</span>
          <span class="panel-count-divider" aria-hidden="true">·</span>
          <span>{{ snapshot.modelCount }} 个模型</span>
        </p>
      </div>
    </section>

    <section v-else class="panel-results" aria-label="模型探测结果">
      <table class="panel-result-table" aria-label="模型状态与耗时">
        <colgroup>
          <col class="panel-model-column" />
          <col class="panel-status-column" />
          <col class="panel-duration-column" />
        </colgroup>
        <tbody
          v-for="site in visibleSites"
          :key="site.siteId"
          class="panel-site-group"
          :data-site-id="site.siteId"
          :aria-label="`${site.siteName}的模型`"
        >
          <tr class="panel-group-spacer" aria-hidden="true">
            <td colspan="3" />
          </tr>
          <tr
            v-for="(model, index) in site.models"
            :key="model.id"
            class="panel-model-row"
            :class="{
              'panel-group-first': index === 0,
              'panel-group-last': index === site.models.length - 1,
            }"
            :data-model-id="model.id"
          >
            <td class="panel-model-cell">
              <a-tooltip :title="model.modelName" :trigger="['hover', 'focus']" placement="topLeft">
                <span class="panel-model-name" tabindex="0">{{ model.modelName }}</span>
              </a-tooltip>
              <a-tooltip :title="site.siteName" :trigger="['hover', 'focus']" placement="topLeft">
                <span class="panel-site-name" tabindex="0">{{ site.siteName }}</span>
              </a-tooltip>
            </td>
            <td>
              <a-tooltip
                :title="
                  model.status === 'pending' ? '已完成探测，尚未达到健康判定阈值。' : undefined
                "
                :trigger="['hover', 'focus']"
              >
                <span
                  class="panel-model-status"
                  :data-status="model.status"
                  :tabindex="model.status === 'pending' ? 0 : undefined"
                >
                  <span class="panel-model-status-dot" aria-hidden="true" />
                  {{ panelModelLabels[model.status] }}
                </span>
              </a-tooltip>
            </td>
            <td class="panel-duration">
              <a-tooltip title="首 Token / 总耗时" :trigger="['hover', 'focus']">
                <span class="panel-duration-pair" tabindex="0">
                  <span
                    :aria-label="model.firstTokenMs == null ? '未提供首 Token 耗时' : undefined"
                  >
                    {{ formatPanelDuration(model.firstTokenMs) }}
                  </span>
                  <span class="panel-duration-separator" aria-hidden="true">/</span>
                  <span :aria-label="model.totalMs == null ? '未提供总耗时' : undefined">
                    {{ formatPanelDuration(model.totalMs) }}
                  </span>
                </span>
              </a-tooltip>
            </td>
          </tr>
        </tbody>
      </table>
      <div class="panel-list-footer">
        <span class="panel-list-caption">
          显示 {{ visibleModelCount }}/{{ snapshot.modelCount }} 个模型
        </span>
        <a-button type="link" size="small" class="panel-view-all" @click="emit('viewAll')">
          查看全部
          <AppIcon name="ArrowRightOutlined" aria-hidden="true" />
        </a-button>
      </div>
    </section>

    <footer class="panel-footer">
      <span class="panel-completion">
        <span>最近完成</span>
        <time
          v-if="snapshot.lastProbe"
          :datetime="snapshot.lastProbe.completedAt"
          :title="panelTimeTitle(snapshot.lastProbe.completedAt)"
          >{{ formatPanelTime(snapshot.lastProbe.completedAt, 'second') }}</time
        >
        <span v-else aria-label="尚未完成首次探测">—</span>
      </span>
      <span class="panel-schedule" role="status" aria-live="polite" aria-atomic="true">
        <span v-if="probing" class="panel-probing">正在探测</span>
        <template v-else-if="snapshot.schedule.state === 'scheduled'">
          <span>下次</span>
          <time
            :datetime="snapshot.schedule.nextProbeAt"
            :title="panelTimeTitle(snapshot.schedule.nextProbeAt)"
            >{{ formatPanelTime(snapshot.schedule.nextProbeAt) }}</time
          >
        </template>
        <span v-else>未启用周期探测</span>
      </span>
    </footer>
  </article>
</template>

<style scoped>
.panel-surface {
  /* 默认实色兜底；分隔色叠加到底色上，避免毛玻璃上的亮白线。 */
  --panel-surface-color: var(--ant-color-bg-container);
  --panel-separator-color: color-mix(in srgb, var(--ant-color-text-base) 5%, transparent);
  --panel-outline-color: color-mix(in srgb, var(--ant-color-text-base) 8%, transparent);
  --panel-group-color: color-mix(in srgb, var(--ant-color-text-base) 2%, transparent);
  width: 360px;
  max-width: 100%;
  overflow: hidden;
  border: 1px solid var(--panel-outline-color);
  border-radius: 12px;
  background: var(--panel-surface-color);
  color: var(--ant-color-text);
  font-size: 12px;
  line-height: 1.5;
  container-type: inline-size;
}

.panel-surface[data-frosted='true'] {
  /* 仅叠加在宿主的系统模糊层上；CSS backdrop-filter 无法模糊其他桌面窗口。 */
  --panel-surface-color: color-mix(in srgb, var(--ant-color-bg-container) 94%, transparent);
}

.panel-header,
.panel-brand,
.panel-actions,
.panel-counts,
.panel-footer,
.panel-completion,
.panel-schedule {
  display: flex;
  align-items: center;
}

.panel-header {
  justify-content: space-between;
  gap: 12px;
  min-height: 54px;
  padding: 10px 14px 10px 16px;
  border-bottom: 1px solid var(--panel-separator-color);
}

.panel-brand {
  min-width: 0;
  gap: 9px;
}

.panel-logo {
  display: grid;
  flex: none;
  width: 24px;
  height: 24px;
  place-items: center;
  border-radius: 6px;
  background: var(--ant-color-primary);
  color: var(--ant-color-text-light-solid);
  font-size: 14px;
}

.panel-name {
  font-size: 14px;
  font-weight: 650;
  letter-spacing: -0.2px;
}

.panel-actions {
  flex: none;
  gap: 4px;
}

.panel-icon-button.ant-btn {
  width: 28px;
  height: 28px;
  color: var(--ant-color-text-secondary);
  font-size: 15px;
}

.panel-summary {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 24px 16px;
  text-align: center;
}

.panel-status-mark {
  display: grid;
  flex: none;
  width: 40px;
  height: 40px;
  place-items: center;
  border: 1px solid var(--panel-separator-color);
  border-radius: 50%;
  background: var(--panel-group-color);
  color: var(--ant-color-text-tertiary);
  font-size: 20px;
}

.panel-summary-copy {
  min-width: 0;
}

.panel-status-title {
  margin: 10px 0 0;
  color: var(--ant-color-text);
  font-size: 16px;
  font-weight: 600;
  line-height: 1.5;
}

.panel-waiting-hint {
  margin: 4px 0 0;
  color: var(--ant-color-text-secondary);
  font-size: 12px;
}

.panel-counts {
  justify-content: center;
  gap: 8px;
  margin: 6px 0 0;
  color: var(--ant-color-text-secondary);
  font-size: 12px;
}

.panel-count-divider {
  color: var(--ant-color-text-quaternary);
}

.panel-results {
  padding: 4px 16px 0;
}

.panel-result-table {
  width: 100%;
  border-spacing: 0;
  border-collapse: separate;
  table-layout: fixed;
}

.panel-model-column {
  width: auto;
}

.panel-status-column {
  width: 48px;
}

.panel-duration-column {
  width: 116px;
}

.panel-group-spacer td {
  height: 8px;
  padding: 0;
  border: 0;
}

.panel-model-row td {
  padding: 10px 0;
  border-bottom: 1px solid var(--panel-separator-color);
  background: var(--panel-group-color);
  vertical-align: middle;
}

.panel-model-row .panel-model-cell {
  padding-inline: 8px;
  border-left: 1px solid var(--panel-separator-color);
}

.panel-model-row td:last-child {
  padding-right: 8px;
  border-right: 1px solid var(--panel-separator-color);
}

.panel-group-first td {
  border-top: 1px solid var(--panel-separator-color);
}

.panel-group-first td:first-child {
  border-top-left-radius: 6px;
}

.panel-group-first td:last-child {
  border-top-right-radius: 6px;
}

.panel-group-last td:first-child {
  border-bottom-left-radius: 6px;
}

.panel-group-last td:last-child {
  border-bottom-right-radius: 6px;
}

.panel-model-name,
.panel-site-name {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel-model-name {
  color: var(--ant-color-text);
  font-size: 12px;
  font-weight: 500;
  line-height: 19px;
}

.panel-site-name {
  margin-top: 3px;
  color: var(--ant-color-text-tertiary);
  font-size: 11px;
  font-weight: 400;
  line-height: 16px;
}

.panel-model-status {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 3px;
  color: var(--ant-color-error-text);
  font-size: 11px;
  white-space: nowrap;
}

.panel-model-status[data-status='slow'] {
  color: var(--ant-color-warning-text);
}

.panel-model-status[data-status='healthy'] {
  color: var(--ant-color-success-text);
}

.panel-model-status[data-status='pending'] {
  color: var(--ant-color-text-secondary);
}

.panel-model-status-dot {
  flex: none;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: currentColor;
}

.panel-duration {
  color: var(--ant-color-text-secondary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  text-align: right;
  white-space: nowrap;
}

.panel-duration-pair {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.panel-duration-pair [aria-label],
.panel-duration-separator {
  color: var(--ant-color-text-quaternary);
}

.panel-list-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 5px 0 12px;
}

.panel-list-caption {
  color: var(--ant-color-text-tertiary);
  font-size: 10px;
}

.panel-view-all.ant-btn {
  height: 26px;
  padding: 0;
  font-size: 11px;
}

.panel-view-all .anticon {
  font-size: 10px;
}

.panel-footer {
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px;
  border-top: 1px solid var(--panel-separator-color);
  background: transparent;
  color: var(--ant-color-text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.panel-completion,
.panel-schedule {
  gap: 4px;
  white-space: nowrap;
}

.panel-probing {
  color: var(--ant-color-primary);
}

@container (max-width: 300px) {
  .panel-results {
    padding-inline: 12px;
  }

  .panel-footer {
    padding-inline: 12px;
  }
}

@media (prefers-reduced-transparency: reduce) {
  .panel-surface[data-frosted='true'] {
    --panel-surface-color: var(--ant-color-bg-container);
  }
}
</style>
