import type {
  OperationLogDetail,
  OperationLogDetailContent,
  OperationLogListItem,
} from '@/types/domain/operation-logs';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';

import * as api from '@/api/run/operation-logs';
import { takeOperationLogView } from '@/views/run/operation-history/operation-log-navigation';

import OperationLogDetailDrawer from '@/views/run/operation-history/components/OperationLogDetailDrawer.vue';
import OperationHistoryView from '@/views/run/operation-history/OperationHistoryView.vue';

// 只替换浮层容器；详情 SFC、字段展示映射和列表按钮仍使用实际实现。
const DrawerContainer = defineComponent({
  props: { open: Boolean },
  template: '<div v-if="open"><slot name="title" /><slot /></div>',
});
const disposals: (() => void)[] = [];
const historyPath = '/run/operation-history';
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  takeOperationLogView(historyPath);
  vi.restoreAllMocks();
});

function historicalDetail(
  content: OperationLogDetailContent,
  action = 'warehouse.batch_completed',
): OperationLogDetail {
  return {
    item: {
      id: 'historical-event',
      occurredAt: '2026-11-09T00:00:00Z',
      source: 'user',
      actorSnapshot: '历史操作者',
      action,
      actionGroup: 'batch',
      actionLabel: '批次完成',
      objectSnapshot: '历史批次',
      result: 'success',
      summary: '批次处理已完成',
      detailBehavior: { kind: 'drawer' },
    },
    explanation: '事件发生时保存的详情',
    content,
  };
}

function mountDetail(content: OperationLogDetailContent, action?: string) {
  const wrapper = mount(OperationLogDetailDrawer, {
    props: {
      open: true,
      loading: false,
      error: '',
      missing: false,
      detail: historicalDetail(content, action),
    },
    global: { plugins: [AntdvNext], stubs: { ADrawer: DrawerContainer } },
  });
  disposals.push(() => wrapper.unmount());
  return wrapper.get('[aria-label="本次事件内容"]');
}

describe('操作详情实际渲染', () => {
  it.each(['warehouse.batch.v7', 'unregistered-format'])(
    '通用格式 %s 显示提供的标签和值',
    (format) => {
      const content = mountDetail({
        kind: 'presentation',
        format,
        fields: [
          { label: 'name', value: '历史批次 A' },
          { label: 'enabled', value: 'true' },
          { label: '提交内容.name', value: '<b>原始文本</b>' },
        ],
      });

      expect(content.text()).toContain('历史批次 A');
      expect(content.text()).toContain('name');
      expect(content.text()).toContain('enabled');
      expect(content.text()).toContain('true');
      expect(content.text()).toContain('提交内容.name');
      expect(content.text()).toContain('<b>原始文本</b>');
      expect(content.find('b').exists()).toBe(false);
      expect(content.text()).not.toMatch(/未生成探测记录|站点名称|小面板显示/u);
    },
  );

  it('通用字段使用同一敏感遮蔽和长文本折叠规则，格式与动作均不触发猜测翻译', () => {
    const longText = '历史正文\n'.repeat(60);
    const content = mountDetail(
      {
        kind: 'presentation',
        format: 'any.presentation',
        fields: [
          { label: 'enabled', value: 'true' },
          { label: '正文', value: longText },
          { label: '凭据', value: 'private-value'.repeat(30), sensitive: true },
          { label: '配置状态', value: '已更换', sensitive: true },
        ],
      },
      'site_created',
    );

    expect(content.text()).toContain('enabled');
    expect(content.text()).toContain('true');
    expect(content.text()).not.toContain('小面板显示');
    expect(content.text()).not.toContain('private-value');
    expect(content.text()).toContain('已隐藏');
    expect(content.text()).toContain('已更换');
    expect(content.findAll('details')).toHaveLength(1);
    expect(content.get('details').attributes('open')).toBeUndefined();
    expect(content.get('details summary').text()).toBe('展开长内容');
    expect(content.get('details pre').element.textContent).toBe(longText);
  });

  describe.each(['warehouse.batch_completed', 'notification_external_archive'])(
    '未知动作 %s',
    (action) => {
      it.each(['snapshot', 'changes', 'deletion'] as const)('%s 不套用既有业务词典', (kind) => {
        const fields = [
          { label: 'name', value: '历史批次' },
          { label: 'enabled', value: 'true' },
          { label: '提交内容.name', value: '原始路径' },
          { label: 'titleTemplate', value: 'null' },
        ];
        let content: OperationLogDetailContent;
        if (kind === 'changes') {
          content = {
            kind,
            changes: fields.map(({ label, value }) => ({
              field: label,
              before: 'false',
              after: value,
            })),
          };
        } else if (kind === 'deletion') {
          content = { kind, summary: '批次已删除', fields };
        } else {
          content = { kind, fields };
        }
        const rendered = mountDetail(content, action);

        for (const field of fields) {
          expect(rendered.text()).toContain(field.label);
          expect(rendered.text()).toContain(field.value);
        }
        expect(rendered.text()).not.toMatch(/站点名称|小面板显示|提交内容 ·|未生成探测记录/u);
      });
    },
  );

  it.each([
    ['site_updated', 'name', '历史站点', '站点名称', '历史站点'],
    ['site_updated', 'enabled', 'true', '小面板显示', '显示'],
    ['general_settings_updated', 'theme', 'dark', '主题', '深色'],
    ['notification_settings_updated', 'notificationsEnabled', 'false', '启用消息通知', '已停用'],
    ['notification_template_updated', 'titleTemplate', 'true', '标题', 'true'],
  ])('既有动作 %s 的字段 %s 保留原有翻译', (action, label, value, title, presented) => {
    const content = mountDetail({ kind: 'snapshot', fields: [{ label, value }] }, action);
    expect(content.text()).toContain(title);
    expect(content.get('pre').text()).toBe(presented);
  });

  it('只有明确的旧探针失败内容展示未生成探测记录', () => {
    const probe = mountDetail({ kind: 'probe_failure', reason: '探测启动失败' }, 'manual_probe');
    expect(probe.text()).toContain('未生成探测记录');
    expect(probe.text()).toContain('探测启动失败');
    const failure = mountDetail({
      kind: 'failure',
      activity: '导入失败',
      target: '历史批次',
      reason: '格式错误',
    });
    expect(failure.text()).toContain('导入失败');
    expect(failure.text()).not.toContain('未生成探测记录');
  });
});

async function mountHistory(detail: OperationLogDetail) {
  vi.spyOn(api, 'fetchOperationLogs').mockResolvedValue({
    items: [detail.item],
    actionGroups: [detail.item.actionGroup],
    total: 1,
    page: 1,
    pageSize: 20,
  });
  const fetchDetail = vi.spyOn(api, 'fetchOperationLogDetail').mockResolvedValue(detail);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: historyPath, name: 'operation-history', component: OperationHistoryView },
      { path: '/run/records', name: 'records', component: { template: '<div />' } },
    ],
  });
  await router.push(historyPath);
  await router.isReady();
  const wrapper = mount(OperationHistoryView, {
    global: { plugins: [AntdvNext, router], stubs: { ADrawer: DrawerContainer } },
  });
  disposals.push(() => wrapper.unmount());
  await flushPromises();
  return { wrapper, router, fetchDetail };
}

describe('操作记录详情导航', () => {
  it.each(['https://example.invalid/admin', 'probe_records'])(
    '通用 Navigate %s 安全回退到实际详情',
    async (target) => {
      const detail = historicalDetail({
        kind: 'presentation',
        format: 'warehouse.batch.v7',
        fields: [{ label: '批次编号', value: '历史批次 A' }],
      });
      detail.item.detailBehavior = {
        kind: 'navigate',
        target,
        parameters: { redirect: '/admin', recordIds: '["foreign-id"]' },
      };
      const { wrapper, router, fetchDetail } = await mountHistory(detail);

      await wrapper.get('button[aria-label="查看操作详情"]').trigger('click');
      await flushPromises();

      expect(fetchDetail).toHaveBeenCalledExactlyOnceWith(detail.item.id);
      expect(router.currentRoute.value.fullPath).toBe(historyPath);
      const content = wrapper.get('[aria-label="本次事件内容"]');
      expect(content.text()).toContain('批次编号');
      expect(content.text()).toContain('历史批次 A');
      expect(content.text()).not.toContain('未生成探测记录');
      expect(wrapper.find('a[href="/admin"]').exists()).toBe(false);
    },
  );

  it.each(['available', 'expired'] as const)(
    '旧探针关联 %s 保持原导航和记录标识',
    async (availability) => {
      const detail = historicalDetail(
        { kind: 'probe_failure', reason: '旧探针详情' },
        'manual_probe',
      );
      detail.item.detailBehavior = {
        kind: 'probe_records',
        availability,
        recordIds: ['historical-1', 'historical-2'],
      };
      const { wrapper, router, fetchDetail } = await mountHistory(detail);

      await wrapper.get('button[aria-label="查看关联探测记录"]').trigger('click');
      await flushPromises();

      expect(router.currentRoute.value.name).toBe('records');
      expect(router.currentRoute.value.query).toEqual({
        recordIds: 'historical-1,historical-2',
        fromOperationHistory: '1',
      });
      expect(fetchDetail).not.toHaveBeenCalled();
    },
  );

  it('旧探针没有关联记录时保留详情失败原因', async () => {
    const detail = historicalDetail(
      { kind: 'probe_failure', reason: '请求尚未建立' },
      'manual_probe',
    );
    const behavior: OperationLogListItem['detailBehavior'] = {
      kind: 'probe_records',
      availability: 'none',
      recordIds: [],
    };
    detail.item.detailBehavior = behavior;
    const { wrapper, router } = await mountHistory(detail);

    await wrapper.get('button[aria-label="查看操作详情"]').trigger('click');
    await flushPromises();

    expect(router.currentRoute.value.fullPath).toBe(historyPath);
    expect(wrapper.get('[aria-label="本次事件内容"]').text()).toContain('请求尚未建立');
  });
});
