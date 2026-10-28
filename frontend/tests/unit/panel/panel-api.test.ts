import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { fetchGeneralSettings, updateGeneralSettings } from '@/api/config/general';
import { resetEventBusForTests } from '@/api/events';
import {
  fetchPanelSnapshot,
  openPanelSettings,
  openPanelSites,
  PANEL_STATE_CHANGED,
  refreshPanel,
  subscribePanelUpdates,
  syncPanelWindow,
} from '@/api/panel';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { createPanelMock } from '@/mocks/panel';

const tauriState = vi.hoisted(() => ({
  tauri: false,
  label: 'status-panel',
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauriState.invoke,
  isTauri: () => tauriState.tauri,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: tauriState.listen, emit: vi.fn() }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: tauriState.label }),
}));

beforeEach(() => {
  tauriState.tauri = false;
  tauriState.label = 'status-panel';
  tauriState.invoke.mockReset();
  tauriState.listen.mockReset();
});
afterEach(() => {
  vi.useRealTimers();
  resetFeedbackForTests();
  resetEventBusForTests();
});

describe('小面板 API 环境分流', () => {
  it('查看全部仅由独立桌面面板调用站点管理动作', async () => {
    await openPanelSites();
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.tauri = true;
    tauriState.label = 'main';
    await openPanelSites();
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.label = 'status-panel';
    tauriState.invoke.mockResolvedValueOnce(undefined);
    await openPanelSites();
    expect(tauriState.invoke).toHaveBeenCalledExactlyOnceWith('open_panel_sites', undefined);
  });

  it('只有面板内明确的设置动作才调用原生主窗口入口', async () => {
    await openPanelSettings();
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.tauri = true;
    tauriState.label = 'main';
    await openPanelSettings();
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.label = 'status-panel';
    tauriState.invoke.mockResolvedValueOnce(undefined);
    await openPanelSettings();
    expect(tauriState.invoke).toHaveBeenCalledExactlyOnceWith('open_panel_settings', undefined);
  });

  it('仅独立原生面板同步内容高度，浏览器和主窗口不触发原生尺寸操作', async () => {
    expect(await syncPanelWindow(265, 'system')).toBe(false);
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.tauri = true;
    tauriState.label = 'main';
    expect(await syncPanelWindow(265, 'system')).toBe(false);
    expect(tauriState.invoke).not.toHaveBeenCalled();
    tauriState.label = 'status-panel';
    tauriState.invoke.mockResolvedValueOnce(true);
    expect(await syncPanelWindow(265, 'dark')).toBe(true);
    expect(tauriState.invoke).toHaveBeenCalledExactlyOnceWith('sync_panel_window', {
      height: 265,
      theme: 'dark',
    });
    tauriState.invoke.mockResolvedValueOnce(false);
    expect(await syncPanelWindow(265, 'system')).toBe(false);
  });

  it('浏览器面板快照与通用设置 mock 使用同一个主题偏好', async () => {
    const settings = await fetchGeneralSettings();
    try {
      await updateGeneralSettings({ ...settings, theme: 'dark' });
      expect((await fetchPanelSnapshot()).theme).toBe('dark');
      await updateGeneralSettings({ ...settings, theme: 'light' });
      expect((await fetchPanelSnapshot()).theme).toBe('light');
    } finally {
      await updateGeneralSettings(settings);
    }
  });

  it('浏览器经注册表读取和刷新 mock，返回的快照不共享可变数据', async () => {
    const waiting = await fetchPanelSnapshot();
    expect(waiting.state).toBe('waiting');
    waiting.siteCount = 999;
    expect((await fetchPanelSnapshot()).siteCount).toBe(4);

    vi.useFakeTimers();
    const pending = refreshPanel();
    await vi.advanceTimersByTimeAsync(1800);
    const completed = await pending;
    expect(completed.state).toBe('ready');
    expect(completed.activeProbe).toBeNull();
    expect(completed.lastProbe?.completedAt).toBe(new Date().toISOString());
    completed.sites[0].models[0].modelName = '已修改的快照';
    expect((await fetchPanelSnapshot()).sites[0].models[0].modelName).not.toBe('已修改的快照');
    expect(tauriState.invoke).not.toHaveBeenCalled();
  });

  it('Tauri 经统一调用层发送读取和刷新命令，直接使用后端返回值', async () => {
    tauriState.tauri = true;
    const waiting = createPanelMock('waiting');
    const completed = createPanelMock('ready');
    tauriState.invoke.mockResolvedValueOnce(waiting).mockResolvedValueOnce(completed);

    expect(await fetchPanelSnapshot()).toEqual(waiting);
    expect(await refreshPanel()).toEqual(completed);
    expect(tauriState.invoke).toHaveBeenNthCalledWith(1, 'get_panel_snapshot', undefined);
    expect(tauriState.invoke).toHaveBeenNthCalledWith(2, 'refresh_panel', undefined);
  });

  it('Tauri 事件明确订阅当前面板目标，隐藏保留监听，显示补读，销毁统一清理', async () => {
    tauriState.tauri = true;
    const stopChanges = vi.fn();
    const stopFocus = vi.fn();
    tauriState.listen.mockResolvedValueOnce(stopChanges).mockResolvedValueOnce(stopFocus);
    const changed = vi.fn();
    const shown = vi.fn();
    const stop = await subscribePanelUpdates(changed, shown);
    expect(tauriState.listen).toHaveBeenNthCalledWith(
      1,
      PANEL_STATE_CHANGED,
      expect.any(Function),
      { target: { kind: 'WebviewWindow', label: 'status-panel' } },
    );
    expect(tauriState.listen).toHaveBeenNthCalledWith(2, 'tauri://focus', expect.any(Function), {
      target: { kind: 'Window', label: 'status-panel' },
    });
    tauriState.listen.mock.calls[0][1]({ payload: { revision: 12 } });
    expect(changed).toHaveBeenCalledWith({ revision: 12 });
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(shown).not.toHaveBeenCalled();
    expect(stopChanges).not.toHaveBeenCalled();
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
    document.dispatchEvent(new Event('visibilitychange'));
    tauriState.listen.mock.calls[1][1]({ payload: null });
    expect(shown).toHaveBeenCalledTimes(2);
    stop();
    expect(stopChanges).toHaveBeenCalledOnce();
    expect(stopFocus).toHaveBeenCalledOnce();
    document.dispatchEvent(new Event('visibilitychange'));
    window.dispatchEvent(new Event('pageshow'));
    expect(shown).toHaveBeenCalledTimes(2);
  });

  it('显示监听注册失败时释放已经注册的业务监听，允许完整重试', async () => {
    tauriState.tauri = true;
    const stopChanges = vi.fn();
    tauriState.listen.mockResolvedValueOnce(stopChanges).mockRejectedValueOnce({
      code: 'listen_failed',
      category: 'unavailable',
      message: 'focus subscription failed',
      retryable: true,
    });
    await expect(subscribePanelUpdates(vi.fn(), vi.fn())).rejects.toThrow(
      'focus subscription failed',
    );
    expect(stopChanges).toHaveBeenCalledOnce();
  });
});
