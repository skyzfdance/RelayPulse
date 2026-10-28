import { invoke } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { callBackend } from '@/api/call-backend';
import { exportDiagnostics } from '@/api/config/general';
import {
  type FeedbackInstances,
  type FeedbackKind,
  type FeedbackMode,
  bindFeedbackInstances,
  resetFeedbackForTests,
} from '@/composables/useFeedback';
import { resetGlobalLoadingForTests, useGlobalLoading } from '@/composables/useGlobalLoading';
import { clearMockHandlers, registerMockHandler } from '@/mocks/handlers/registry';
import { RelayPulseBackendError } from '@/utils/backend-error';

const tauriState = vi.hoisted(() => ({ tauri: false, invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauriState.invoke,
  isTauri: () => tauriState.tauri,
}));

interface ShownFeedback {
  mode: FeedbackMode;
  kind: FeedbackKind;
  payload: Record<string, unknown>;
}

function createFakeInstances() {
  const shown: ShownFeedback[] = [];
  const record = (mode: FeedbackMode, kind: FeedbackKind) => (payload: Record<string, unknown>) => {
    shown.push({ mode, kind, payload });
    // 模拟 antd 展示后关闭（message 自动消失 / modal 即时确认），驱动单例队列继续出队；
    // 否则 current 一直被占用，后续同类提示会卡在队列里不展示。
    const dismiss = (payload.onClose ?? payload.onOk) as (() => void) | undefined;
    dismiss?.();
  };
  const kinds: FeedbackKind[] = ['success', 'error', 'info', 'warning'];
  const instances = {
    message: Object.fromEntries(kinds.map((kind) => [kind, record('message', kind)])),
    notification: Object.fromEntries(kinds.map((kind) => [kind, record('notification', kind)])),
    modal: Object.fromEntries(kinds.map((kind) => [kind, record('modal', kind)])),
  } as unknown as FeedbackInstances;
  return { shown, instances };
}

let fake: ReturnType<typeof createFakeInstances>;

beforeEach(() => {
  clearMockHandlers();
  resetFeedbackForTests();
  resetGlobalLoadingForTests();
  tauriState.tauri = false;
  tauriState.invoke.mockReset();
  fake = createFakeInstances();
  bindFeedbackInstances(fake.instances);
});

afterEach(() => {
  clearMockHandlers();
  resetFeedbackForTests();
  resetGlobalLoadingForTests();
  vi.useRealTimers();
});

describe('callBackend 统一调用层', () => {
  it('取消导出诊断包时按后端文案弹一次错误 modal，不提示成功', async () => {
    tauriState.tauri = true;
    tauriState.invoke.mockRejectedValue({
      code: 'diagnostics_export_cancelled',
      category: 'cancelled',
      message: '已取消导出诊断包。',
      field: null,
      retryable: false,
      operationId: null,
    });

    await expect(exportDiagnostics()).rejects.toMatchObject({
      code: 'diagnostics_export_cancelled',
      message: '已取消导出诊断包。',
    });
    expect(tauriState.invoke).toHaveBeenCalledWith('export_diagnostics', undefined);
    expect(fake.shown).toHaveLength(1);
    expect(fake.shown[0]).toMatchObject({
      mode: 'modal',
      kind: 'error',
      payload: { content: '已取消导出诊断包。' },
    });
  });

  it('浏览器模式：按注册表分发并透传参数', async () => {
    registerMockHandler('demo_query', (args) => (args as { x: number }).x * 2);

    await expect(
      callBackend<number>('demo_query', { x: 21 }, { successFeedback: 'none' }),
    ).resolves.toBe(42);
    expect(fake.shown).toHaveLength(0);
  });

  it('浏览器模式：未注册命令 reject，且默认 failFeedback=modal 展示错误', async () => {
    await expect(callBackend('missing_cmd')).rejects.toBeInstanceOf(RelayPulseBackendError);

    expect(fake.shown).toHaveLength(1);
    expect(fake.shown[0]).toMatchObject({ mode: 'modal', kind: 'error' });
  });

  it('成功默认弹 message「操作成功」，文案可覆盖', async () => {
    registerMockHandler('demo_save', () => 'ok');

    await expect(callBackend('demo_save')).resolves.toBe('ok');
    expect(fake.shown[0]).toMatchObject({ mode: 'message', kind: 'success' });
    expect(fake.shown[0].payload.content).toBe('操作成功');

    await callBackend('demo_save', undefined, { successText: '站点已保存' });
    expect(fake.shown[1].payload.content).toBe('站点已保存');
  });

  it('failFeedback: none 时静默 reject，调用方自行处理', async () => {
    await expect(
      callBackend('missing_cmd', undefined, { failFeedback: 'none', successFeedback: 'none' }),
    ).rejects.toBeInstanceOf(RelayPulseBackendError);
    expect(fake.shown).toHaveLength(0);
  });

  it('Tauri 模式：走 invoke 并透传命令与参数', async () => {
    tauriState.tauri = true;
    tauriState.invoke.mockResolvedValue('from-tauri');

    await expect(callBackend('get_sites', undefined, { successFeedback: 'none' })).resolves.toBe(
      'from-tauri',
    );
    expect(tauriState.invoke).toHaveBeenCalledWith('get_sites', undefined);
    expect(invoke).toBeDefined();
  });

  it('loading: true 占用全局遮罩，结束后按最短停留隐藏', async () => {
    vi.useFakeTimers();
    let resolveSlow!: (value: string) => void;
    registerMockHandler(
      'demo_slow',
      () => new Promise<string>((resolve) => (resolveSlow = resolve)),
    );

    const pending = callBackend<string>('demo_slow', undefined, {
      loading: true,
      successFeedback: 'none',
    });
    await vi.advanceTimersByTimeAsync(200);
    expect(useGlobalLoading().visible.value).toBe(true);

    resolveSlow('done');
    await vi.advanceTimersByTimeAsync(0);
    expect(useGlobalLoading().visible.value).toBe(true); // 最短停留未到
    await vi.advanceTimersByTimeAsync(300);
    expect(useGlobalLoading().visible.value).toBe(false);

    await expect(pending).resolves.toBe('done');
  });
});
