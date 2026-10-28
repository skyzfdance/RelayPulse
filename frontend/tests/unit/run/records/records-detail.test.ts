import type { ProbeRecordDetail } from '@/types/domain/records';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Drawer } from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, ref } from 'vue';

import * as api from '@/api/run/records';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { createProbeRecordFixtures } from '@/mocks/handlers/run/records-fixtures';
import { RelayPulseBackendError } from '@/utils/backend-error';
import { useProbeRecordDetail } from '@/views/run/records/use-probe-record-detail';

import { deferred } from '../../../support/deferred';

import ProbeRecordDetailDrawer from '@/views/run/records/components/ProbeRecordDetailDrawer.vue';

const disposals: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/run/records');
  seedProbeRecords();
});
afterEach(() => {
  disposals
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  vi.restoreAllMocks();
  window.history.replaceState({}, '', '/');
});
function mountDrawer(recordId: string | null) {
  const wrapper = mount(ProbeRecordDetailDrawer, {
    props: { recordId },
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  disposals.push(() => wrapper.unmount());
  return wrapper;
}
function content() {
  return new DOMWrapper(document.querySelector<HTMLElement>('[aria-label="探测详情内容"]')!);
}
async function waitForLoaded() {
  await vi.waitFor(() => expect(content().attributes('aria-busy')).toBe('false'));
  await flushPromises();
}
function fields() {
  return Object.fromEntries(
    content()
      .findAll('dl > div')
      .map((row) => [row.get('dt').text(), row.get('dd').text()]),
  );
}

describe('单条探测详情', () => {
  it('结果合并为一行，按保存的阈值解释当次偏慢', async () => {
    mountDrawer('probe-001');
    await waitForLoaded();
    expect(content().get('[aria-label="本次请求结果"]').text()).toBe('响应偏慢');
    expect(
      content()
        .findAll('p')
        .filter((item) => item.text() === '响应偏慢'),
    ).toHaveLength(0);
    const basis = content().get('[aria-label="当次偏慢判定依据"]');
    expect(basis.text()).toContain('当次快照');
    expect(basis.text()).toContain('首 Token 38.74 s ≥ 30 s（阈值）');
    expect(basis.text()).toContain('总耗时 39.87 s < 60 s（阈值）');
  });

  it.each([
    [1000, 999, 'gte', 'lt', '首 Token 1 s ≥ 1 s（阈值）', '总耗时 999 ms < 1 s（阈值）'],
    [999, 1000, 'lt', 'gte', '首 Token 999 ms < 1 s（阈值）', '总耗时 1 s ≥ 1 s（阈值）'],
    [9999, 10_000, 'lt', 'gte', '首 Token 9.999 s < 10 s（阈值）', '总耗时 10 s ≥ 10 s（阈值）'],
  ] as const)(
    '格式化接口给出的比较，保留毫秒精度：%s / %s',
    async (firstTokenMs, durationMs, firstComparison, totalComparison, first, total) => {
      const detail = await api.fetchProbeRecordDetail('probe-001');
      vi.spyOn(api, 'fetchProbeRecordDetail').mockResolvedValue({
        ...detail,
        firstTokenMs,
        durationMs,
        slowThresholds: {
          firstTokenMs: durationMs === 10_000 ? 10_000 : 1000,
          durationMs: durationMs === 10_000 ? 10_000 : 1000,
        },
        slowEvaluation: {
          status: 'evaluated',
          firstToken: { comparison: firstComparison, triggered: firstComparison === 'gte' },
          duration: { comparison: totalComparison, triggered: totalComparison === 'gte' },
        },
      });
      mountDrawer(detail.id);
      await waitForLoaded();
      const basis = content().get('[aria-label="当次偏慢判定依据"]').text();
      expect(basis).toContain(first);
      expect(basis).toContain(total);
    },
  );

  it('比较与触发项只使用接口结论，不根据耗时重新推导', async () => {
    const detail = await api.fetchProbeRecordDetail('probe-001');
    // 故意提供与比较结论不一致的数值，验证页面不会自行纠正后端数据。
    vi.spyOn(api, 'fetchProbeRecordDetail').mockResolvedValue({
      ...detail,
      firstTokenMs: 1,
      durationMs: 100_000,
    });
    mountDrawer(detail.id);
    await waitForLoaded();
    const basis = content().get('[aria-label="当次偏慢判定依据"]');
    expect(basis.text()).toContain('首 Token 1 ms ≥ 30 s（阈值）');
    expect(basis.text()).toContain('总耗时 100 s < 60 s（阈值）');
    const checks = basis.findAll('span');
    expect(checks[0].classes()).toContain('text-(--ant-color-warning)');
    expect(checks[1].classes()).not.toContain('text-(--ant-color-warning)');
  });

  it('缺失阈值或耗时不伪造比较，不按当前配置重建缺失的判定快照', async () => {
    const detail = await api.fetchProbeRecordDetail('probe-001');
    vi.spyOn(api, 'fetchProbeRecordDetail').mockResolvedValueOnce({
      ...detail,
      firstTokenMs: null,
      durationMs: 1000,
      slowThresholds: { firstTokenMs: null, durationMs: 1000 },
      slowEvaluation: {
        status: 'evaluated',
        firstToken: { comparison: null, triggered: false },
        duration: { comparison: 'gte', triggered: true },
      },
    });
    const wrapper = mountDrawer(detail.id);
    await waitForLoaded();
    expect(content().get('[aria-label="当次偏慢判定依据"]').text()).toMatch(
      /首 Token —\s+· 阈值 —/,
    );
    expect(content().get('[aria-label="当次偏慢判定依据"]').text()).toContain(
      '总耗时 1 s ≥ 1 s（阈值）',
    );
    await wrapper.setProps({ recordId: 'probe-019' });
    await waitForLoaded();
    expect(content().get('[aria-label="当次偏慢判定依据"]').text()).toBe('未保存当次判定快照');
    expect(content().get('[aria-label="本次请求结果"]').text()).toBe('请求成功');
  });

  it('失败优先；阈值只解释历史结果，不在前端重新判定已保存的结果', async () => {
    const detail = await api.fetchProbeRecordDetail('probe-002');
    vi.spyOn(api, 'fetchProbeRecordDetail').mockResolvedValue({
      ...detail,
      durationMs: 60_000,
      slowThresholds: { firstTokenMs: 1000, durationMs: 1000 },
    });
    mountDrawer(detail.id);
    await waitForLoaded();
    expect(content().get('[aria-label="本次请求结果"]').text()).toBe(detail.resultMessage);
    const basis = content().get('[aria-label="当次偏慢判定依据"]');
    expect(basis.text()).toContain('请求失败，未参与偏慢判定');
    expect(basis.text()).not.toContain('≥');
    expect(basis.text()).not.toContain('响应偏慢');
  });

  it.each(['provider', 'unreported'] as const)(
    '用量来源 %s 直接取接口字段，不根据 Token 是否存在推导',
    async (usageSource) => {
      const detail = await api.fetchProbeRecordDetail('probe-001');
      vi.spyOn(api, 'fetchProbeRecordDetail').mockResolvedValue({
        ...detail,
        usageSource,
        usage:
          usageSource === 'provider' ? null : { inputTokens: 0, outputTokens: 0, totalTokens: 0 },
      });
      mountDrawer(detail.id);
      await waitForLoaded();
      expect(fields().用量来源).toBe(usageSource === 'provider' ? '供应商上报' : '未上报');
      expect(fields()['总 Token']).toBe(usageSource === 'provider' ? '—' : '0');
    },
  );

  it.each([
    ['probe-010', '0', '0', '0', '供应商上报'],
    ['probe-009', '640', '—', '—', '供应商上报'],
    ['probe-012', '—', '—', '—', '未上报'],
  ])(
    '详情 %s 保留明确零值和部分上报，缺失数据不补成零',
    async (id, input, output, total, source) => {
      mountDrawer(id);
      await waitForLoaded();
      expect(fields()).toMatchObject({
        '输入 Token': input,
        '输出 Token': output,
        '总 Token': total,
        用量来源: source,
      });
      if (id === 'probe-012') {
        expect(fields()).toMatchObject({
          '首 Token': '—',
          总耗时: '—',
          'HTTP 状态': '—',
          '请求 ID': '—',
        });
      }
      expect(content().findAll('input,textarea')).toHaveLength(0);
      expect(content().text()).toContain('请求与响应');
      expect(content().find('[aria-label="请求体快照"]').exists()).toBe(false);
    },
  );

  it('缺失的端点与策略显示占位，长文本按原内容展示', async () => {
    const first = createProbeRecordFixtures()[0];
    seedProbeRecords([
      {
        ...first,
        siteId: 'unknown-site',
        siteName: '很长的历史站点名称'.repeat(8),
        resultMessage: '<script>不作为 HTML 执行</script>',
      },
    ]);
    mountDrawer(first.id);
    await waitForLoaded();
    expect(fields()).toMatchObject({ 请求端点: '—', 探针策略: '—' });
    expect(content().text()).toContain('<script>不作为 HTML 执行</script>');
    expect(content().find('script').exists()).toBe(false);
  });

  it('读取中可以关闭；关闭后到达的响应不会残留到下一次打开', async () => {
    const first = await api.fetchProbeRecordDetail('probe-001');
    const pending = deferred<ProbeRecordDetail>();
    const read = vi.spyOn(api, 'fetchProbeRecordDetail').mockReturnValueOnce(pending.promise);
    const wrapper = mountDrawer(null);
    expect(read).not.toHaveBeenCalled();
    await wrapper.setProps({ recordId: first.id });
    expect(content().attributes('aria-busy')).toBe('true');
    expect(content().find('[role="status"]').exists()).toBe(true);
    expect(content().text()).not.toContain('38.74 s');
    const close = document.querySelector('.ant-drawer-close')!;
    await new DOMWrapper(close).trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    await wrapper.setProps({ recordId: null });
    pending.resolve(first);
    await flushPromises();
    expect(wrapper.getComponent(Drawer).props('open')).toBe(false);
    await wrapper.setProps({ recordId: 'probe-002' });
    await waitForLoaded();
    expect(content().text()).toContain('模型服务返回了服务器错误');
    expect(content().text()).not.toContain('38.74 s');
  });

  it.each(['成功', '失败'] as const)(
    '切换记录后，前一条较晚%s不会覆盖当前详情',
    async (outcome) => {
      const first = await api.fetchProbeRecordDetail('probe-001');
      const second = await api.fetchProbeRecordDetail('probe-002');
      const old = deferred<ProbeRecordDetail>();
      const current = deferred<ProbeRecordDetail>();
      vi.spyOn(api, 'fetchProbeRecordDetail')
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const wrapper = mountDrawer(first.id);
      await wrapper.setProps({ recordId: second.id });
      current.resolve(second);
      await flushPromises();
      expect(fields()['HTTP 状态']).toBe('503');
      if (outcome === '成功') old.resolve(first);
      else old.reject(new Error('较早的读取失败'));
      await flushPromises();
      expect(fields()['HTTP 状态']).toBe('503');
      expect(content().text()).not.toContain('较早的读取失败');
      expect(content().text()).not.toContain('38.74 s');
    },
  );

  it('失败在抽屉内显示且可以重试，不存在的记录显示空态而不伪造详情', async () => {
    vi.spyOn(api, 'fetchProbeRecordDetail').mockRejectedValueOnce(
      new RelayPulseBackendError({
        code: 'probe_records_unavailable',
        category: 'unavailable',
        message: '详情暂时不可用',
        retryable: true,
      }),
    );
    const wrapper = mountDrawer('probe-001');
    await flushPromises();
    expect(content().get('[role="alert"]').text()).toContain('详情暂时不可用');
    expect(content().find('dl').exists()).toBe(false);
    await content().get('button').trigger('click');
    await waitForLoaded();
    expect(fields()['HTTP 状态']).toBe('200');
    await wrapper.setProps({ recordId: 'not-found' });
    await waitForLoaded();
    expect(content().text()).toContain('这条探测记录已不存在或已过期');
    expect(content().find('dl').exists()).toBe(false);
    expect(content().find('[role="alert"]').exists()).toBe(false);
  });

  it('离开页面后，未完成的读取不再写入详情状态', async () => {
    const first = await api.fetchProbeRecordDetail('probe-001');
    const pending = deferred<ProbeRecordDetail>();
    vi.spyOn(api, 'fetchProbeRecordDetail').mockReturnValueOnce(pending.promise);
    const scope = effectScope();
    const state = scope.run(() => useProbeRecordDetail(ref(first.id)))!;
    scope.stop();
    pending.resolve(first);
    await flushPromises();
    expect(state.detail.value).toBeUndefined();
    expect(state.error.value).toBe('');
  });
});
