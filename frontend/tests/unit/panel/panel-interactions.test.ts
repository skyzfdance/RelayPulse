import type { PanelSnapshot } from '@/types/domain/panel';

import { enableAutoUnmount, flushPromises, shallowMount } from '@vue/test-utils';
import { Button } from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import {
  fetchPanelSnapshot,
  openPanelSettings,
  openPanelSites,
  refreshPanel,
  subscribePanelUpdates,
} from '@/api/panel';
import { createPanelMock } from '@/mocks/panel';
import { useThemeStore } from '@/stores/theme';

import { deferred } from '../../support/deferred';

import PanelSurface from '@/views/panel/components/PanelSurface.vue';
import PanelView from '@/views/panel/PanelView.vue';

vi.mock('@/api/panel', () => ({
  fetchPanelSnapshot: vi.fn(),
  refreshPanel: vi.fn(),
  subscribePanelUpdates: vi.fn(),
  syncPanelWindow: vi.fn(),
  openPanelSettings: vi.fn(),
  openPanelSites: vi.fn(),
}));

let changed: Parameters<typeof subscribePanelUpdates>[0];
let shown: () => void;
const unlisten = vi.fn();

enableAutoUnmount(afterEach);
beforeEach(() => {
  setActivePinia(createPinia());
  vi.mocked(openPanelSettings).mockReset().mockResolvedValue(undefined);
  vi.mocked(openPanelSites).mockReset().mockResolvedValue(undefined);
  vi.mocked(fetchPanelSnapshot).mockReset().mockResolvedValue(createPanelMock('waiting'));
  vi.mocked(refreshPanel).mockReset().mockResolvedValue(createPanelMock('ready'));
  unlisten.mockReset();
  vi.mocked(subscribePanelUpdates)
    .mockReset()
    .mockImplementation(async (onChange, onShow) => {
      changed = onChange;
      shown = onShow;
      return unlisten;
    });
});

describe('小面板 API 交互', () => {
  it('打开及重新聚焦时落在面板容器，内部控件仍可聚焦，卸载后不再接管焦点', async () => {
    const wrapper = shallowMount(PanelView, {
      attachTo: document.body,
      global: {
        stubs: {
          PanelSurface: { template: '<button aria-label="立即探测">刷新</button>' },
        },
      },
    });
    await flushPromises();
    const panel = wrapper.get('main').element;
    const refresh = wrapper.get('button').element;
    expect(document.activeElement).toBe(panel);
    refresh.focus();
    expect(document.activeElement).toBe(refresh);
    window.dispatchEvent(new FocusEvent('focus'));
    expect(document.activeElement).toBe(panel);
    expect(refreshPanel).not.toHaveBeenCalled();

    wrapper.unmount();
    const other = document.createElement('button');
    document.body.append(other);
    try {
      other.focus();
      window.dispatchEvent(new FocusEvent('focus'));
      expect(document.activeElement).toBe(other);
    } finally {
      other.remove();
    }
  });

  it('读取与刷新不打开主窗口，查看全部与设置分别调用对应动作', async () => {
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    const panel = wrapper.getComponent(PanelSurface);
    expect(openPanelSettings).not.toHaveBeenCalled();
    expect(openPanelSites).not.toHaveBeenCalled();
    panel.vm.$emit('refresh');
    await flushPromises();
    panel.vm.$emit('viewAll');
    await flushPromises();
    expect(openPanelSites).toHaveBeenCalledOnce();
    expect(openPanelSettings).not.toHaveBeenCalled();
    panel.vm.$emit('openSettings');
    await flushPromises();
    expect(openPanelSettings).toHaveBeenCalledOnce();
    expect(wrapper.emitted('openSettings')).toHaveLength(1);
    expect(wrapper.emitted('viewAll')).toHaveLength(1);
  });

  it('查看全部动作失败保留面板内容，可以再次点击重试', async () => {
    vi.mocked(openPanelSites).mockRejectedValueOnce(new Error('窗口不可用'));
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    wrapper.getComponent(PanelSurface).vm.$emit('viewAll');
    await flushPromises();
    expect(wrapper.findComponent(PanelSurface).exists()).toBe(true);
    wrapper.getComponent(PanelSurface).vm.$emit('viewAll');
    await flushPromises();
    expect(openPanelSites).toHaveBeenCalledTimes(2);
    expect(openPanelSettings).not.toHaveBeenCalled();
  });

  it('初读应用已保存主题，后端失效通知更新主题，system 继续跟随系统外观', async () => {
    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('waiting'),
      theme: 'dark',
    });
    shallowMount(PanelView);
    await flushPromises();
    const theme = useThemeStore();
    expect(theme.mode).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');

    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('waiting'),
      revision: 1,
      theme: 'light',
    });
    changed({ revision: 1 });
    await flushPromises();
    expect(theme.mode).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');

    theme.systemDark = true;
    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('waiting'),
      revision: 2,
      theme: 'system',
    });
    changed({ revision: 2 });
    await flushPromises();
    expect(theme.mode).toBe('system');
    expect(document.documentElement.dataset.theme).toBe('dark');
    theme.systemDark = false;
    await nextTick();
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('首次读取期间保持空内容，不显示加载页或伪造等待快照', async () => {
    const initial = deferred<PanelSnapshot>();
    vi.mocked(fetchPanelSnapshot).mockReturnValueOnce(initial.promise);
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    expect(wrapper.findComponent(PanelSurface).exists()).toBe(false);
    expect(wrapper.text()).toBe('');
    expect(wrapper.find('.panel-load-state').exists()).toBe(false);
    initial.resolve(createPanelMock('waiting'));
    await flushPromises();
    expect(wrapper.getComponent(PanelSurface).props('snapshot').state).toBe('waiting');
  });

  it('通过 API 读取和刷新，等待期间阻止重复请求，直接展示返回的快照', async () => {
    const request = deferred<PanelSnapshot>();
    vi.mocked(refreshPanel).mockReturnValueOnce(request.promise);
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    const panel = wrapper.getComponent(PanelSurface);

    expect(fetchPanelSnapshot).toHaveBeenCalledOnce();
    expect(panel.props('snapshot').state).toBe('waiting');
    panel.vm.$emit('refresh');
    panel.vm.$emit('refresh');
    await nextTick();
    expect(refreshPanel).toHaveBeenCalledOnce();
    expect(panel.props('refreshing')).toBe(true);
    expect(panel.props('snapshot').activeProbe).toBeNull();

    const completed = createPanelMock('ready');
    request.resolve(completed);
    await flushPromises();
    expect(panel.props('snapshot')).toEqual(completed);
    expect(panel.props('refreshing')).toBe(false);
  });

  it('读取失败可重试，刷新失败后恢复按钮并允许再次刷新', async () => {
    vi.mocked(fetchPanelSnapshot).mockRejectedValueOnce(new Error('暂时无法读取面板'));
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe('暂时无法读取面板');
    expect(wrapper.findComponent(PanelSurface).exists()).toBe(false);

    wrapper.getComponent(Button).vm.$emit('click', new MouseEvent('click'));
    await flushPromises();
    expect(fetchPanelSnapshot).toHaveBeenCalledTimes(2);
    const panel = wrapper.getComponent(PanelSurface);
    const previous = panel.props('snapshot');

    vi.mocked(refreshPanel).mockRejectedValueOnce(new Error('刷新失败'));
    panel.vm.$emit('refresh');
    await flushPromises();
    expect(panel.props('snapshot')).toEqual(previous);
    expect(panel.props('refreshing')).toBe(false);

    panel.vm.$emit('refresh');
    await flushPromises();
    expect(refreshPanel).toHaveBeenCalledTimes(2);
    expect(panel.props('snapshot').state).toBe('ready');
  });

  it('监听注册完成后才初读，初读期间的通知合并补读并拒绝旧快照', async () => {
    const listening = deferred<() => void>();
    const initial = deferred<PanelSnapshot>();
    const latest = deferred<PanelSnapshot>();
    vi.mocked(subscribePanelUpdates).mockImplementationOnce((onChange, onShow) => {
      changed = onChange;
      shown = onShow;
      return listening.promise;
    });
    vi.mocked(fetchPanelSnapshot)
      .mockReturnValueOnce(initial.promise)
      .mockReturnValueOnce(latest.promise);
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    expect(fetchPanelSnapshot).not.toHaveBeenCalled();
    changed({ revision: 1 });
    await flushPromises();
    expect(fetchPanelSnapshot).not.toHaveBeenCalled();
    listening.resolve(unlisten);
    await flushPromises();
    expect(fetchPanelSnapshot).toHaveBeenCalledOnce();
    changed({ revision: 2 });
    changed({ revision: 3 });
    initial.resolve(createPanelMock('waiting'));
    await flushPromises();
    expect(wrapper.findComponent(PanelSurface).exists()).toBe(false);
    expect(fetchPanelSnapshot).toHaveBeenCalledTimes(2);
    latest.resolve({ ...createPanelMock('ready'), revision: 3 });
    await flushPromises();
    expect(wrapper.getComponent(PanelSurface).props('snapshot').revision).toBe(3);
  });

  it('刷新期间接收真实任务进度，晚到的旧命令返回不能覆盖更新的事件重读', async () => {
    const refreshing = deferred<PanelSnapshot>();
    vi.mocked(refreshPanel).mockReturnValueOnce(refreshing.promise);
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    const panel = wrapper.getComponent(PanelSurface);
    panel.vm.$emit('refresh');
    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('waiting'),
      revision: 2,
      activeProbe: { completedCount: 0, totalCount: 12 },
    });
    changed({ revision: 2 });
    await flushPromises();
    expect(panel.props('snapshot').activeProbe).toEqual({ completedCount: 0, totalCount: 12 });
    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('ready'),
      revision: 4,
      theme: 'light',
    });
    changed({ revision: 4 });
    await flushPromises();
    refreshing.resolve({ ...createPanelMock('ready'), revision: 3, theme: 'dark' });
    await flushPromises();
    expect(panel.props('snapshot').revision).toBe(4);
    expect(useThemeStore().mode).toBe('light');
    expect(panel.props('snapshot').activeProbe).toBeNull();
    expect(panel.props('refreshing')).toBe(false);
  });

  it('隐藏不卸载订阅，多次显示只补读，销毁后释放监听并忽略晚到的事件和请求', async () => {
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    const panel = wrapper.getComponent(PanelSurface);
    vi.mocked(fetchPanelSnapshot).mockResolvedValueOnce({
      ...createPanelMock('ready'),
      revision: 2,
    });
    shown();
    shown();
    await flushPromises();
    expect(subscribePanelUpdates).toHaveBeenCalledOnce();
    expect(unlisten).not.toHaveBeenCalled();
    expect(panel.props('snapshot').revision).toBe(2);
    const late = deferred<PanelSnapshot>();
    vi.mocked(fetchPanelSnapshot).mockReturnValueOnce(late.promise);
    changed({ revision: 3 });
    await nextTick();
    wrapper.unmount();
    const requests = vi.mocked(fetchPanelSnapshot).mock.calls.length;
    shown();
    changed({ revision: 4 });
    late.resolve({ ...createPanelMock('ready'), revision: 3 });
    await flushPromises();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(fetchPanelSnapshot).toHaveBeenCalledTimes(requests);

    shallowMount(PanelView);
    await flushPromises();
    expect(subscribePanelUpdates).toHaveBeenCalledTimes(2);
  });

  it('订阅尚未注册完成就销毁时立即释放晚到监听，不继续初读', async () => {
    const listening = deferred<() => void>();
    vi.mocked(subscribePanelUpdates).mockReturnValueOnce(listening.promise);
    const wrapper = shallowMount(PanelView);
    wrapper.unmount();
    listening.resolve(unlisten);
    await flushPromises();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(fetchPanelSnapshot).not.toHaveBeenCalled();
  });

  it('订阅失败进入原有重试界面，重试成功后只保留一份监听', async () => {
    vi.mocked(subscribePanelUpdates).mockRejectedValueOnce(new Error('暂时无法订阅面板'));
    const wrapper = shallowMount(PanelView);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe('暂时无法订阅面板');
    expect(fetchPanelSnapshot).not.toHaveBeenCalled();
    wrapper.getComponent(Button).vm.$emit('click', new MouseEvent('click'));
    await flushPromises();
    expect(wrapper.getComponent(PanelSurface).props('snapshot').state).toBe('waiting');
    expect(fetchPanelSnapshot).toHaveBeenCalledOnce();
    expect(subscribePanelUpdates).toHaveBeenCalledTimes(2);
    wrapper.unmount();
    expect(unlisten).toHaveBeenCalledOnce();
  });
});
