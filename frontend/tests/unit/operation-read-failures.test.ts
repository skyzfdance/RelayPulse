import { flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, ref } from 'vue';

import { callBackend } from '@/api/call-backend';
import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { useSiteDetailsStore, useSiteStore } from '@/stores/site';
import { RelayPulseBackendError } from '@/utils/backend-error';
import { useProbeRecordDetail } from '@/views/run/records/use-probe-record-detail';
import { useProbeRecordRequest } from '@/views/run/records/use-probe-record-request';
import { useProbeRecordResponse } from '@/views/run/records/use-probe-record-response';
import { useTokenStats } from '@/views/run/records/use-token-stats';

import { deferred } from '../support/deferred';

const native = vi.hoisted(() => ({ tauri: true, invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: native.invoke,
  isTauri: () => native.tauri,
}));
const disposals: (() => void)[] = [];
const reports = () =>
  native.invoke.mock.calls.filter(([command]) => command === 'report_operation_read_failure');
const payload = {
  code: 'storage_unavailable',
  category: 'storage',
  message: '读取失败',
  retryable: true,
};

beforeEach(() => {
  native.tauri = true;
  native.invoke.mockReset().mockResolvedValue(undefined);
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
});

describe('前端可感知读取失败的受限桥接', () => {
  it('只发送固定目标和类别，不发送异常、路由或请求参数', () => {
    reportOperationReadFailure('site_list', 'page', new Error('secret in URL/header'));
    reportOperationReadFailure('probe_record_detail', 'detail', new Error('secret body'));
    expect(reports()).toEqual([
      ['report_operation_read_failure', { input: { target: 'site_list', kind: 'page' } }],
      [
        'report_operation_read_failure',
        { input: { target: 'probe_record_detail', kind: 'detail' } },
      ],
    ]);
  });

  it('跳过后端已知失败，但保留归一化后的前端异常来源', () => {
    reportOperationReadFailure('site_list', 'page', payload);
    reportOperationReadFailure('site_list', 'page', new RelayPulseBackendError(payload));
    expect(reports()).toHaveLength(0);
    reportOperationReadFailure(
      'site_list',
      'page',
      new RelayPulseBackendError(payload, 'frontend'),
    );
    expect(reports()).toHaveLength(1);
  });

  it('通信层只归一化并保留错误来源，不自行推断业务事件', async () => {
    native.invoke.mockRejectedValue(new Error('transport failed'));
    await expect(
      callBackend('get_sites', undefined, { successFeedback: 'none', failFeedback: 'none' }),
    ).rejects.toMatchObject({ origin: 'frontend' });
    expect(native.invoke.mock.calls.map(([command]) => command)).toEqual([
      'get_sites',
      'record_frontend_error',
    ]);
    expect(reports()).toHaveLength(0);
  });

  it('浏览器不写真实后端，桥接自身失败不递归或影响调用方', async () => {
    native.tauri = false;
    reportOperationReadFailure('probe_records', 'page', new Error('failed'));
    expect(native.invoke).not.toHaveBeenCalled();
    native.tauri = true;
    native.invoke.mockRejectedValue(new Error('bridge failed'));
    reportOperationReadFailure('probe_records', 'page', new Error('original'));
    await flushPromises();
    expect(native.invoke).toHaveBeenCalledTimes(1);
  });
});

describe('读取失败由仍可见的详情用例采集', () => {
  it.each(['关闭详情', '离开页面'])('%s 后旧请求失败不再上报', async (action) => {
    const pending = deferred<never>();
    native.invoke.mockImplementation((command: string) =>
      command === 'get_probe_record_detail' ? pending.promise : Promise.resolve(),
    );
    const scope = effectScope();
    disposals.push(() => scope.stop());
    const id = ref<string | null>('record-1');
    const detail = scope.run(() => useProbeRecordDetail(id))!;
    if (action === '关闭详情') id.value = null;
    else scope.stop();
    await flushPromises();
    pending.reject(new Error('late transport failure'));
    await flushPromises();
    expect(detail.error.value).toBe('');
    expect(reports()).toHaveLength(0);
  });

  it('两次独立可见重试各上报一次，归一化不丢失前端异常来源', async () => {
    native.invoke.mockImplementation((command: string) =>
      command === 'get_probe_record_detail'
        ? Promise.reject(new Error('transport failed'))
        : Promise.resolve(),
    );
    const scope = effectScope();
    disposals.push(() => scope.stop());
    const detail = scope.run(() => useProbeRecordDetail(ref('record-1')))!;
    await flushPromises();
    expect(detail.error.value).not.toBe('');
    expect(reports()).toHaveLength(1);
    await detail.reload();
    expect(reports()).toHaveLength(2);
  });

  it('后端返回结构化业务失败时由后端采集，详情错误可见也不再补报', async () => {
    native.invoke.mockImplementation((command: string) =>
      command === 'get_probe_record_detail' ? Promise.reject(payload) : Promise.resolve(),
    );
    const scope = effectScope();
    disposals.push(() => scope.stop());
    const detail = scope.run(() => useProbeRecordDetail(ref('record-1')))!;
    await flushPromises();
    expect(detail.error.value).toBe(payload.message);
    expect(reports()).toHaveLength(0);
  });
});

describe('隐藏错误在真正展示时采集', () => {
  it.each(['request', 'response'] as const)(
    '%s 收起期间的失败在重新展开时只补报一次',
    async (kind) => {
      const pending = deferred<never>();
      native.invoke.mockImplementation((command: string) =>
        command === `get_probe_record_${kind}` ? pending.promise : Promise.resolve(),
      );
      const scope = effectScope();
      disposals.push(() => scope.stop());
      const open = ref(true);
      const useContent = kind === 'request' ? useProbeRecordRequest : useProbeRecordResponse;
      const state = scope.run(() => useContent(ref('record-1'), open))!;
      open.value = false;
      await flushPromises();
      pending.reject(new Error('hidden transport failure'));
      await flushPromises();
      expect(state.error.value).not.toBe('');
      expect(reports()).toHaveLength(0);
      open.value = true;
      await flushPromises();
      expect(reports()).toHaveLength(1);
      open.value = false;
      await flushPromises();
      open.value = true;
      await flushPromises();
      expect(reports()).toHaveLength(1);
    },
  );

  it('隐藏页签预读失败在切换到该页签时才补报，来回切换不重复', async () => {
    native.invoke.mockImplementation((command: string) =>
      command === 'get_probe_token_stats'
        ? Promise.reject(new Error('hidden tab failure'))
        : Promise.resolve(),
    );
    const scope = effectScope();
    disposals.push(() => scope.stop());
    const visible = ref(false);
    const state = scope.run(() =>
      useTokenStats({ range: ref('24h'), siteId: ref('') }, () => visible.value),
    )!;
    await state.load();
    expect(state.error.value).not.toBe('');
    expect(reports()).toHaveLength(0);
    visible.value = true;
    await flushPromises();
    expect(reports()).toHaveLength(1);
    visible.value = false;
    await flushPromises();
    visible.value = true;
    await flushPromises();
    expect(reports()).toHaveLength(1);
  });

  it('共享站点 Store 保留状态，但离开站点页面后的详情失败不采集', async () => {
    setActivePinia(createPinia());
    const store = useSiteStore();
    store.sites = [
      {
        id: 'site-1',
        name: '站点',
        priority: 1,
        enabled: true,
        modelCount: 0,
        health: 'muted',
        statusText: '暂无模型',
      },
    ];
    store.sitesLoaded = true;
    store.selectedSiteId = 'site-1';
    store.viewActive = true;
    const pending = deferred<never>();
    native.invoke.mockImplementation((command: string) =>
      command === 'get_site' ? pending.promise : Promise.resolve(),
    );
    const details = useSiteDetailsStore();
    const reading = details.loadSiteDetails();
    store.viewActive = false;
    pending.reject(new Error('late site read'));
    await reading;
    expect(reports()).toHaveLength(0);
    store.viewActive = true;
    await details.loadSiteDetails();
    expect(reports()).toHaveLength(1);
  });

  it('打开原始日志目录的前端异常在有效错误提示处上报，结构化失败不重复', async () => {
    native.invoke.mockImplementation((command: string) => {
      if (command === 'get_probe_record_response')
        return Promise.resolve({ logFile: { canOpenDirectory: true } });
      if (command === 'open_probe_record_log_directory')
        return Promise.reject(new Error('frontend open failure'));
      return Promise.resolve();
    });
    const scope = effectScope();
    disposals.push(() => scope.stop());
    const state = scope.run(() => useProbeRecordResponse(ref('record-1'), ref(true)))!;
    await flushPromises();
    await state.openLogDirectory();
    expect(reports()).toHaveLength(1);
    native.invoke.mockImplementation((command: string) =>
      command === 'open_probe_record_log_directory' ? Promise.reject(payload) : Promise.resolve(),
    );
    await state.openLogDirectory();
    expect(reports()).toHaveLength(1);
  });
});
