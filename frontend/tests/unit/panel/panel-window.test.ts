import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, ref } from 'vue';

import { syncPanelWindow } from '@/api/panel';
import { usePanelWindow } from '@/views/panel/usePanelWindow';

import { deferred } from '../../support/deferred';

vi.mock('@/api/panel', () => ({ syncPanelWindow: vi.fn() }));

let height = 0;
let theme = ref<ThemeMode>('system');
let resized: () => void;
const disconnect = vi.fn();
const originalResizeObserver = globalThis.ResizeObserver;
const Host = defineComponent({
  setup() {
    const content = ref<HTMLElement | null>(null);
    const { nativeBackdrop } = usePanelWindow(content, theme);
    return () => h('main', { ref: content, 'data-frosted': nativeBackdrop.value });
  },
});

enableAutoUnmount(afterEach);
beforeEach(() => {
  height = 0;
  theme = ref<ThemeMode>('system');
  disconnect.mockReset();
  vi.mocked(syncPanelWindow).mockReset().mockResolvedValue(false);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(() => ({
    ...new DOMRect(),
    height,
  }));
  globalThis.ResizeObserver = class implements ResizeObserver {
    constructor(callback: ResizeObserverCallback) {
      resized = () => callback([], this);
    }
    observe() {}
    unobserve() {}
    disconnect = disconnect;
  };
});
afterEach(() => {
  globalThis.ResizeObserver = originalResizeObserver;
});

describe('小面板内容高度与原生显示同步', () => {
  it('空内容不显示窗口，实际内容就绪后同步高度，后续只随内容变化', async () => {
    const wrapper = mount(Host);
    await flushPromises();
    expect(syncPanelWindow).not.toHaveBeenCalled();
    expect(wrapper.attributes('data-frosted')).toBe('false');
    height = 264.2;
    resized();
    await flushPromises();
    expect(syncPanelWindow).toHaveBeenCalledExactlyOnceWith(265, 'system');
    resized();
    await flushPromises();
    expect(syncPanelWindow).toHaveBeenCalledOnce();
    expect(wrapper.attributes('data-frosted')).toBe('false');
    vi.mocked(syncPanelWindow).mockResolvedValueOnce(true);
    height = 312;
    resized();
    await flushPromises();
    expect(syncPanelWindow).toHaveBeenLastCalledWith(312, 'system');
    expect(wrapper.attributes('data-frosted')).toBe('true');
  });

  it('串行同步期间合并尺寸变化，卸载后不再发送晚到的更新', async () => {
    const initial = deferred<boolean>();
    vi.mocked(syncPanelWindow).mockReturnValueOnce(initial.promise);
    height = 265;
    const wrapper = mount(Host);
    await flushPromises();
    height = 300;
    resized();
    height = 320;
    resized();
    theme.value = 'dark';
    await nextTick();
    expect(syncPanelWindow).toHaveBeenCalledOnce();
    initial.resolve(true);
    await flushPromises();
    expect(vi.mocked(syncPanelWindow).mock.calls).toEqual([
      [265, 'system'],
      [320, 'dark'],
    ]);
    wrapper.unmount();
    height = 400;
    resized();
    await flushPromises();
    expect(syncPanelWindow).toHaveBeenCalledTimes(2);
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it('内容高度不变时也同步主题，回到 system 时清除原生主题覆盖', async () => {
    height = 265;
    mount(Host);
    await flushPromises();
    for (const mode of ['light', 'dark', 'system'] as const) {
      theme.value = mode;
      await flushPromises();
      expect(syncPanelWindow).toHaveBeenLastCalledWith(265, mode);
    }
    expect(syncPanelWindow).toHaveBeenCalledTimes(4);
  });

  it('宿主尚未确认系统模糊时保持实色，确认后才启用半透明材质', async () => {
    const native = deferred<boolean>();
    vi.mocked(syncPanelWindow).mockReturnValueOnce(native.promise);
    height = 265;
    const wrapper = mount(Host);
    await flushPromises();
    expect(wrapper.attributes('data-frosted')).toBe('false');
    native.resolve(true);
    await flushPromises();
    expect(wrapper.attributes('data-frosted')).toBe('true');
  });
});
import type { ThemeMode } from '@/types/theme';
