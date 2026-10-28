import type { ProbeRecordRequestContent } from '@/types/domain/records';
import type { VueWrapper } from '@vue/test-utils';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Collapse } from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as api from '@/api/run/records';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { RelayPulseBackendError } from '@/utils/backend-error';
import { formatBytes } from '@/utils/format-bytes';

import { deferred } from '../../../support/deferred';

import ProbeRecordRequestSection from '@/views/run/records/components/ProbeRecordRequestSection.vue';

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
async function mountSection(recordId = 'probe-001') {
  const detail = await api.fetchProbeRecordDetail(recordId);
  const wrapper = mount(ProbeRecordRequestSection, {
    props: { recordId, summary: detail.requestSummary, responseSummary: detail.responseSummary },
    global: { plugins: [AntdvNext] },
  });
  disposals.push(() => wrapper.unmount());
  return wrapper;
}
function requestToggle(wrapper: VueWrapper) {
  return wrapper.findAll('[role="button"][aria-expanded]')[0];
}
async function waitForLoaded(wrapper: VueWrapper) {
  await vi.waitFor(() =>
    expect(wrapper.get('[aria-label="请求详情内容"]').attributes('aria-busy')).toBe('false'),
  );
  await flushPromises();
}

describe('探测记录的请求详情', () => {
  it('两块默认收起，首次展开才读取请求，收起后再次展开复用同一份快照', async () => {
    const read = vi.spyOn(api, 'fetchProbeRecordRequest');
    const wrapper = await mountSection();
    const collapse = wrapper.getComponent(Collapse);
    expect(collapse.props('activeKey')).toEqual([]);
    expect(collapse.props('expandIconPlacement')).toBe('end');
    expect(wrapper.text()).toContain('原始返回');
    expect(read).not.toHaveBeenCalled();
    expect(wrapper.find('pre').exists()).toBe(false);
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(read).toHaveBeenCalledExactlyOnceWith('probe-001');
    const request = await api.fetchProbeRecordRequest('probe-001');
    read.mockClear();
    expect(wrapper.get('pre code').text()).toBe(request.body);
    expect(wrapper.text()).toContain(`POST · ${formatBytes(request.bodyBytes!)}`);
    expect(wrapper.findAll('h4').map((heading) => heading.text())).toEqual([
      '请求头（敏感字段已脱敏）',
      '请求体快照',
    ]);
    expect(wrapper.get('[aria-label="已脱敏请求头"]').text()).toContain('[REDACTED]');
    expect(wrapper.text()).not.toContain('原始请求体');
    await requestToggle(wrapper).trigger('click');
    expect(collapse.props('activeKey')).toEqual([]);
    await requestToggle(wrapper).trigger('click');
    await flushPromises();
    expect(read).not.toHaveBeenCalled();
    const response = wrapper.findAll('[role="button"][aria-expanded]')[1];
    expect(response.attributes('aria-disabled')).not.toBe('true');
    expect(response.attributes('aria-expanded')).toBe('false');
  });

  it('加载中允许收起；再次展开不重复读取，完成后按原文本展示', async () => {
    const request = await api.fetchProbeRecordRequest('probe-001');
    const pending = deferred<ProbeRecordRequestContent>();
    const read = vi.spyOn(api, 'fetchProbeRecordRequest').mockReturnValueOnce(pending.promise);
    const wrapper = await mountSection();
    await requestToggle(wrapper).trigger('click');
    expect(wrapper.get('[aria-label="请求详情内容"]').attributes('aria-busy')).toBe('true');
    expect(wrapper.find('[role="status"]').exists()).toBe(true);
    await requestToggle(wrapper).trigger('click');
    await requestToggle(wrapper).trigger('click');
    expect(read).toHaveBeenCalledTimes(1);
    pending.resolve(request);
    await waitForLoaded(wrapper);
    expect(wrapper.get('pre code').text()).toBe(request.body);
  });

  it('读取失败留在面板内，可重试错误通过重新加载恢复', async () => {
    const read = vi.spyOn(api, 'fetchProbeRecordRequest').mockRejectedValueOnce(
      new RelayPulseBackendError({
        code: 'probe_records_unavailable',
        category: 'unavailable',
        message: '请求内容暂时无法读取',
        retryable: true,
      }),
    );
    const wrapper = await mountSection();
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.get('[role="alert"]').text()).toContain('请求内容暂时无法读取');
    expect(wrapper.find('pre').exists()).toBe(false);
    await wrapper.get('button').trigger('click');
    await waitForLoaded(wrapper);
    expect(read).toHaveBeenCalledTimes(2);
    expect(wrapper.get('pre').text()).toContain('只回复 OK');
  });

  it('内容文件不可用时不伪装成空请求，也不提示无效重试', async () => {
    const wrapper = await mountSection();
    window.history.replaceState({}, '', '/run/records?recordsScenario=request-missing');
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.get('[role="alert"]').text()).toContain('未找到这条记录的请求内容文件');
    expect(wrapper.find('pre').exists()).toBe(false);
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('未保存、明确为空与 0 字节分别表达，不从空值伪造内容', async () => {
    const wrapper = await mountSection('probe-019');
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.text()).toContain('POST · —');
    expect(wrapper.text()).toContain('未保存请求头');
    expect(wrapper.text()).toContain('未保存请求体');
    expect(wrapper.find('pre').exists()).toBe(false);
    const legacy = await api.fetchProbeRecordDetail('probe-020');
    await wrapper.setProps({ recordId: legacy.id, summary: legacy.requestSummary });
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.text()).toContain('未保存请求头');
    expect(wrapper.get('[aria-label="请求体快照"]').text()).toContain('只回复 OK');
    vi.spyOn(api, 'fetchProbeRecordRequest').mockResolvedValue({
      method: 'POST',
      bodyBytes: 0,
      headers: [],
      body: '',
    });
    await wrapper.setProps({ recordId: 'empty-body', summary: { method: 'POST', bodyBytes: 0 } });
    expect(wrapper.getComponent(Collapse).props('activeKey')).toEqual([]);
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.text()).toContain('POST · 0 B');
    expect(wrapper.text()).toContain('无请求头');
    expect(wrapper.text()).toContain('请求体为空');
    expect(wrapper.text()).not.toContain('未保存请求头');
    expect(wrapper.text()).not.toContain('未保存请求体');
  });

  it('保留后端提供的重复请求头、长文本和请求体空白，均作为文本展示', async () => {
    const body = ` {\n  "input": "${'你好🙂<script>示例</script>'.repeat(80)}"\n}\n`;
    const headers = [
      { name: 'x-example', value: '<script>请求头示例</script>'.repeat(20) },
      { name: 'x-example', value: '同名请求头的第二个值' },
    ];
    vi.spyOn(api, 'fetchProbeRecordRequest').mockResolvedValue({
      method: 'POST',
      bodyBytes: new TextEncoder().encode(body).byteLength,
      headers,
      body,
    });
    const wrapper = await mountSection();
    await requestToggle(wrapper).trigger('click');
    await waitForLoaded(wrapper);
    expect(wrapper.findAll('dt').map((name) => name.text())).toEqual(
      headers.map((header) => header.name),
    );
    expect(wrapper.findAll('dd').map((value) => value.text())).toEqual(
      headers.map((header) => header.value),
    );
    expect(wrapper.get('[aria-label="请求体快照"]').element.textContent).toBe(body);
    expect(wrapper.find('script').exists()).toBe(false);
  });

  it.each(['成功', '失败'] as const)(
    '换记录时恢复折叠，旧请求较晚%s不串入新记录',
    async (outcome) => {
      const first = await api.fetchProbeRecordRequest('probe-001');
      const second = await api.fetchProbeRecordRequest('probe-002');
      const old = deferred<ProbeRecordRequestContent>();
      const current = deferred<ProbeRecordRequestContent>();
      const read = vi
        .spyOn(api, 'fetchProbeRecordRequest')
        .mockReturnValueOnce(old.promise)
        .mockReturnValueOnce(current.promise);
      const wrapper = await mountSection();
      await requestToggle(wrapper).trigger('click');
      await wrapper.setProps({
        recordId: 'probe-002',
        summary: { method: 'POST', bodyBytes: second.bodyBytes },
      });
      expect(wrapper.getComponent(Collapse).props('activeKey')).toEqual([]);
      expect(read).toHaveBeenCalledTimes(1);
      await requestToggle(wrapper).trigger('click');
      current.resolve(second);
      await waitForLoaded(wrapper);
      if (outcome === '成功') old.resolve(first);
      else old.reject(new Error('旧请求错误'));
      await flushPromises();
      expect(wrapper.get('pre code').text()).toBe(second.body);
      expect(wrapper.text()).not.toContain('旧请求错误');
    },
  );
});
