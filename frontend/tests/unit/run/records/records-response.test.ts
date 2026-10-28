import type { OpenProbeRecordLogResult, ProbeRecordResponseContent } from '@/types/domain/records';
import type { VueWrapper } from '@vue/test-utils';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Collapse } from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as api from '@/api/run/records';
import { bindFeedbackInstances, resetFeedbackForTests } from '@/composables/useFeedback';
import { seedProbeRecords } from '@/mocks/handlers/run/records';
import { RelayPulseBackendError } from '@/utils/backend-error';
import { formatBytes } from '@/utils/format-bytes';

import { deferred } from '../../../support/deferred';

import ProbeRecordRequestSection from '@/views/run/records/components/ProbeRecordRequestSection.vue';

const disposals: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/run/records');
  seedProbeRecords();
  resetFeedbackForTests();
});
afterEach(() => {
  disposals
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
  vi.restoreAllMocks();
  resetFeedbackForTests();
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
function toggle(wrapper: VueWrapper) {
  return wrapper.findAll('[role="button"][aria-expanded]')[1];
}
function panel(wrapper: VueWrapper) {
  return wrapper.get('[aria-label="原始返回内容"]');
}
async function loaded(wrapper: VueWrapper) {
  await vi.waitFor(() => expect(panel(wrapper).attributes('aria-busy')).toBe('false'));
  await flushPromises();
}
function feedbackSpies() {
  const done = (config: { content: string; onClose?: () => void }) => config.onClose?.();
  const success = vi.fn(done);
  const error = vi.fn(done);
  bindFeedbackInstances({
    message: { success, error, info: vi.fn(done), warning: vi.fn(done) },
    notification: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
    modal: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
  });
  return { success, error };
}

describe('原始返回元数据及日志入口', () => {
  it('默认收起、首次展开读取，展示完整大小和分片，收起后复用且不读取合并正文', async () => {
    const expected = await api.fetchProbeRecordResponse('probe-001');
    const read = vi.spyOn(api, 'fetchProbeRecordResponse');
    const requestRead = vi.spyOn(api, 'fetchProbeRecordRequest');
    const wrapper = await mountSection();
    expect(wrapper.getComponent(Collapse).props('activeKey')).toEqual([]);
    expect(toggle(wrapper).text()).toContain(formatBytes(expected.receivedBytes!));
    expect(read).not.toHaveBeenCalled();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(read).toHaveBeenCalledExactlyOnceWith('probe-001');
    expect(requestRead).not.toHaveBeenCalled();
    expect(panel(wrapper).text()).toContain('text/event-stream');
    expect(
      panel(wrapper)
        .findAll('dt')
        .map((entry) => entry.text()),
    ).toEqual(expected.headers!.map((header) => header.name));
    expect(
      panel(wrapper)
        .findAll('dd')
        .map((entry) => entry.text()),
    ).toEqual(expected.headers!.map((header) => header.value));
    expect(panel(wrapper).findAll('button')).toHaveLength(1);
    expect(panel(wrapper).get('button').text()).toBe('打开目录');
    expect(panel(wrapper).text()).toContain('接收数据块（10）');
    expect(panel(wrapper).findAll('li')).toHaveLength(10);
    expect(panel(wrapper).text()).toContain(expected.logFile!.path);
    expect(panel(wrapper).find('pre').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('合并后的原始流');
    expect(wrapper.text()).not.toContain('内容超过保存上限');
    expect(wrapper.text()).not.toContain('64 KB');
    await toggle(wrapper).trigger('click');
    await toggle(wrapper).trigger('click');
    await flushPromises();
    expect(read).toHaveBeenCalledTimes(1);
  });

  it('直接展示后端给出的大小、计数、序号和路径，不从分片求和或重建，长文本安全转义', async () => {
    const original = await api.fetchProbeRecordResponse('probe-001');
    const path = '/mock/<script>日志路径</script>/response.jsonl';
    const headers = [
      { name: 'x-example', value: '<script>响应头示例</script>'.repeat(20) },
      { name: 'x-example', value: '同名响应头的第二个值' },
    ];
    vi.spyOn(api, 'fetchProbeRecordResponse').mockResolvedValue({
      ...original,
      receivedBytes: 1234567,
      headers,
      chunkCount: 77,
      chunks: [{ index: 8, elapsedMs: 0, bytes: 0 }],
      logFile: { path, canOpenDirectory: false },
    });
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(toggle(wrapper).text()).toContain(formatBytes(1234567));
    expect(panel(wrapper).text()).toContain('接收数据块（77）');
    expect(panel(wrapper).get('li').text()).toContain('数据块 8');
    expect(panel(wrapper).get('li').text()).toContain('请求开始后 0 ms 收到');
    expect(panel(wrapper).get('li').text()).toContain('0 B');
    expect(
      panel(wrapper)
        .findAll('dt')
        .map((entry) => entry.text()),
    ).toEqual(headers.map((header) => header.name));
    expect(
      panel(wrapper)
        .findAll('dd')
        .map((entry) => entry.text()),
    ).toEqual(headers.map((header) => header.value));
    expect(panel(wrapper).text()).toContain(path);
    expect(wrapper.find('script').exists()).toBe(false);
    expect(
      panel(wrapper)
        .findAll('button')
        .every((button) => button.attributes('disabled') !== undefined),
    ).toBe(true);
  });

  it.each(['probe-020', 'probe-010', 'probe-012'])(
    '记录 %s 区分未保存、明确零值与没有收到响应，不补造分片',
    async (id) => {
      const wrapper = await mountSection(id);
      await toggle(wrapper).trigger('click');
      await loaded(wrapper);
      if (id === 'probe-020') {
        expect(panel(wrapper).text()).toContain('未保存响应头');
        expect(panel(wrapper).text()).not.toContain('接收数据块');
      } else if (id === 'probe-010') {
        expect(toggle(wrapper).text()).toContain('0 B');
        expect(panel(wrapper).text()).toContain('接收数据块（0）');
        expect(panel(wrapper).text()).toContain('无接收数据块记录');
      } else {
        expect(toggle(wrapper).text()).toContain('未收到响应');
        expect(panel(wrapper).text()).toContain('本次请求未收到响应');
        expect(panel(wrapper).text()).not.toContain('接收数据块');
        expect(toggle(wrapper).text()).not.toContain('0 B');
      }
      expect(panel(wrapper).text()).toContain('日志路径');
      expect(panel(wrapper).findAll('button')).toHaveLength(1);
    },
  );

  it('未知响应和缺失路径显示真实缺失状态，不提供无效打开入口', async () => {
    vi.spyOn(api, 'fetchProbeRecordResponse').mockResolvedValue({
      state: 'unknown',
      kind: null,
      receivedBytes: null,
      headers: null,
      chunkCount: null,
      chunks: null,
      errorResponse: null,
      logFile: null,
    });
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(panel(wrapper).text()).toContain('未保存响应信息');
    expect(panel(wrapper).text()).toContain('未保存日志路径');
    expect(panel(wrapper).text()).not.toContain('未收到响应');
    expect(panel(wrapper).findAll('button')).toHaveLength(0);
  });

  it.each([
    ['probe-002', 503, 'Service temporarily unavailable'],
    ['probe-003', 529, 'Provider is overloaded'],
    ['probe-004', 400, 'Invalid model parameter'],
    ['probe-007', 200, 'The provider rejected this request'],
    ['probe-032', 302, '<html><body>Moved temporarily</body></html>'],
  ] as const)('失败记录 %s（HTTP %s）展示后端给出的响应体并保留日志', async (id, status, text) => {
    const detail = await api.fetchProbeRecordDetail(id);
    const content = await api.fetchProbeRecordResponse(id);
    expect(detail.requestInfo.httpStatus).toBe(status);
    const wrapper = await mountSection(id);
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    const responseBody = panel(wrapper).get('[aria-label="错误响应体"]');
    expect(responseBody.get('pre').text()).toBe(content.errorResponse!.body);
    expect(responseBody.text()).toContain(text);
    expect(panel(wrapper).text()).toContain(content.logFile!.path);
    expect(panel(wrapper).get('button').text()).toBe('打开目录');
    expect(panel(wrapper).find('html').exists()).toBe(false);
    expect(responseBody.text()).not.toContain('响应未接收完整');
  });

  it.each([
    ['probe-017', '响应体为空'],
    ['probe-022', '未保存可展示的响应体，请通过日志查看。'],
    ['probe-027', '响应未接收完整，以下为已保存内容。'],
  ])('失败记录 %s 区分空、未保存及部分响应，不影响日志入口', async (id, text) => {
    const content = await api.fetchProbeRecordResponse(id);
    const wrapper = await mountSection(id);
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    const responseBody = panel(wrapper).get('[aria-label="错误响应体"]');
    expect(responseBody.text()).toContain(text);
    expect(responseBody.find('pre').exists()).toBe(id === 'probe-027');
    expect(panel(wrapper).text()).toContain(content.logFile!.path);
    expect(panel(wrapper).get('button').attributes('disabled')).toBeUndefined();
  });

  it('仅按响应体契约决定展示，保留文本原样并转义，不自行解析业务字段', async () => {
    const original = await api.fetchProbeRecordResponse('probe-001');
    const body = '<script>window.alert("error")</script>\ndata: {"error":{"code":98765}}';
    vi.spyOn(api, 'fetchProbeRecordResponse').mockResolvedValue({
      ...original,
      errorResponse: { body, bodyComplete: null },
    });
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(panel(wrapper).get('pre').text()).toBe(body);
    expect(panel(wrapper).find('script').exists()).toBe(false);
    expect(panel(wrapper).text()).not.toContain('响应未接收完整');
    expect(panel(wrapper).text()).toContain(original.logFile!.path);
  });

  it('加载中可反复收起展开，复用进行中的读取', async () => {
    const response = await api.fetchProbeRecordResponse('probe-001');
    const pending = deferred<ProbeRecordResponseContent>();
    const read = vi.spyOn(api, 'fetchProbeRecordResponse').mockReturnValueOnce(pending.promise);
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    expect(panel(wrapper).attributes('aria-busy')).toBe('true');
    expect(panel(wrapper).get('[role="status"]').attributes('aria-label')).toBe('原始返回加载中');
    await toggle(wrapper).trigger('click');
    await toggle(wrapper).trigger('click');
    expect(read).toHaveBeenCalledTimes(1);
    pending.resolve(response);
    await loaded(wrapper);
    expect(panel(wrapper).text()).toContain(response.logFile!.path);
  });

  it('可重试读取失败通过面板恢复，不伪装成空响应', async () => {
    const read = vi.spyOn(api, 'fetchProbeRecordResponse').mockRejectedValueOnce(
      new RelayPulseBackendError({
        code: 'probe_records_unavailable',
        category: 'unavailable',
        message: '响应日志暂时不可读',
        retryable: true,
      }),
    );
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(panel(wrapper).get('[role="alert"]').text()).toContain('响应日志暂时不可读');
    expect(panel(wrapper).text()).not.toContain('未收到响应');
    await panel(wrapper).get('button').trigger('click');
    await loaded(wrapper);
    expect(read).toHaveBeenCalledTimes(2);
    expect(panel(wrapper).text()).toContain('日志路径');
  });

  it('日志文件缺失显示不可重试错误，不提供伪造的元数据或路径', async () => {
    window.history.replaceState({}, '', '/run/records?recordsScenario=response-missing');
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    expect(panel(wrapper).get('[role="alert"]').text()).toContain('未找到这条记录的响应日志文件');
    expect(panel(wrapper).findAll('button')).toHaveLength(0);
    expect(panel(wrapper).text()).not.toContain('未保存响应头');
  });

  it.each(['成功', '失败'] as const)('换记录后较晚%s的读取不会串入新记录', async (outcome) => {
    const first = await api.fetchProbeRecordResponse('probe-001');
    const second = await api.fetchProbeRecordResponse('probe-002');
    const detail = await api.fetchProbeRecordDetail('probe-002');
    const old = deferred<ProbeRecordResponseContent>();
    const current = deferred<ProbeRecordResponseContent>();
    vi.spyOn(api, 'fetchProbeRecordResponse')
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise);
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await wrapper.setProps({
      recordId: detail.id,
      summary: detail.requestSummary,
      responseSummary: detail.responseSummary,
    });
    expect(wrapper.getComponent(Collapse).props('activeKey')).toEqual([]);
    await toggle(wrapper).trigger('click');
    current.resolve(second);
    await loaded(wrapper);
    if (outcome === '成功') old.resolve(first);
    else old.reject(new Error('旧响应读取失败'));
    await flushPromises();
    expect(panel(wrapper).text()).toContain(second.logFile!.path);
    expect(panel(wrapper).text()).not.toContain(first.logFile!.path);
    expect(panel(wrapper).text()).not.toContain('旧响应读取失败');
  });

  it('打开目录仅提交记录 ID，处理中防止重复打开', async () => {
    const messages = feedbackSpies();
    const pending = deferred<OpenProbeRecordLogResult>();
    const open = vi.spyOn(api, 'openProbeRecordLogDirectory').mockReturnValueOnce(pending.promise);
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    const button = panel(wrapper).get('button');
    await button.trigger('click');
    expect(open).toHaveBeenCalledExactlyOnceWith('probe-001');
    expect(button.attributes('disabled')).toBeDefined();
    await button.trigger('click');
    expect(open).toHaveBeenCalledTimes(1);
    pending.resolve({ message: '浏览器预览：已模拟打开日志目录' });
    await flushPromises();
    expect(messages.success).toHaveBeenCalledWith(
      expect.objectContaining({ content: '浏览器预览：已模拟打开日志目录' }),
    );
    expect(button.attributes('disabled')).toBeUndefined();
  });

  it('打开失败通过现有反馈提示，保留元数据并允许再次打开', async () => {
    const messages = feedbackSpies();
    const open = vi
      .spyOn(api, 'openProbeRecordLogDirectory')
      .mockRejectedValueOnce(new Error('系统无法打开日志目录'));
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    await panel(wrapper).get('button').trigger('click');
    await flushPromises();
    expect(messages.error).toHaveBeenCalledWith(
      expect.objectContaining({ content: '系统无法打开日志目录' }),
    );
    expect(panel(wrapper).text()).toContain('接收数据块');
    await panel(wrapper).get('button').trigger('click');
    await flushPromises();
    expect(open).toHaveBeenCalledTimes(2);
    expect(messages.success).toHaveBeenCalledWith(
      expect.objectContaining({ content: '浏览器预览：已模拟打开日志目录' }),
    );
  });

  it('切换记录后旧打开结果不弹提示，也不解除新打开操作的忙碌状态', async () => {
    const messages = feedbackSpies();
    const detail = await api.fetchProbeRecordDetail('probe-002');
    const old = deferred<OpenProbeRecordLogResult>();
    const current = deferred<OpenProbeRecordLogResult>();
    const open = vi
      .spyOn(api, 'openProbeRecordLogDirectory')
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise);
    const wrapper = await mountSection();
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    await panel(wrapper).get('button').trigger('click');
    await wrapper.setProps({
      recordId: detail.id,
      summary: detail.requestSummary,
      responseSummary: detail.responseSummary,
    });
    await toggle(wrapper).trigger('click');
    await loaded(wrapper);
    await panel(wrapper).get('button').trigger('click');
    expect(open).toHaveBeenLastCalledWith('probe-002');
    old.resolve({ message: '旧日志已打开' });
    await flushPromises();
    expect(messages.success).not.toHaveBeenCalled();
    expect(
      panel(wrapper)
        .findAll('button')
        .every((button) => button.attributes('disabled') !== undefined),
    ).toBe(true);
    current.resolve({ message: '当前日志目录已打开' });
    await flushPromises();
    expect(messages.success).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ content: '当前日志目录已打开' }),
    );
  });
});
