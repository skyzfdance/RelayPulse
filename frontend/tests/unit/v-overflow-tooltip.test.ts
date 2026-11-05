import type { VueWrapper } from '@vue/test-utils';

import { mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, ref, withDirectives } from 'vue';

import { useOverflowTooltip } from '@/composables/useOverflowTooltip';
import { vOverflowTooltip } from '@/directives/overflow-tooltip';

interface Dimensions {
  client: number;
  scroll: number;
}

const mounted = new Set<VueWrapper>();
const originalResizeObserver = globalThis.ResizeObserver;
const tooltipService = useOverflowTooltip();
let dimensions = new WeakMap<Element, Dimensions>();
let defaultDimensions: Dimensions;
let resizeObservers: ManualResizeObserver[];
let mutationObservers: ManualMutationObserver[];
let frames: Map<number, FrameRequestCallback>;
let nextFrameId: number;

class ManualResizeObserver implements ResizeObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();

  constructor(private readonly callback: ResizeObserverCallback) {
    resizeObservers.push(this);
  }

  emit() {
    this.callback([], this);
  }
}

class ManualMutationObserver implements MutationObserver {
  observe = vi.fn();
  disconnect = vi.fn();
  takeRecords = () => [];

  constructor(private readonly callback: MutationCallback) {
    mutationObservers.push(this);
  }

  emit() {
    this.callback([], this);
  }
}

function mountText(
  options: { text?: string; tooltip?: string; title?: string; class?: string } = {},
) {
  const text = ref(options.text ?? '  系统完成站点 doudui.cc 的自动探测。  ');
  const tooltip = ref<string | undefined>(options.tooltip);
  const className = ref(options.class);
  const Host = defineComponent({
    setup: () => () =>
      withDirectives(h('span', { title: options.title, class: className.value }, text.value), [
        [vOverflowTooltip, tooltip.value],
      ]),
  });
  const wrapper = mount(Host);
  mounted.add(wrapper);
  return { wrapper, el: wrapper.element as HTMLElement, text, tooltip, className };
}

function unmount(wrapper: VueWrapper) {
  wrapper.unmount();
  mounted.delete(wrapper);
}

function flushFrame() {
  const pending = [...frames.values()];
  frames.clear();
  pending.forEach((callback) => callback(0));
}

beforeEach(() => {
  dimensions = new WeakMap();
  defaultDimensions = { client: 100, scroll: 250 };
  resizeObservers = [];
  mutationObservers = [];
  frames = new Map();
  nextFrameId = 1;
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    return (dimensions.get(this) ?? defaultDimensions).client;
  });
  vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    return (dimensions.get(this) ?? defaultDimensions).scroll;
  });
  globalThis.ResizeObserver = ManualResizeObserver;
  vi.stubGlobal('MutationObserver', ManualMutationObserver);
  vi.stubGlobal(
    'requestAnimationFrame',
    vi.fn((callback: FrameRequestCallback) => {
      const id = nextFrameId++;
      frames.set(id, callback);
      return id;
    }),
  );
  vi.stubGlobal(
    'cancelAnimationFrame',
    vi.fn((id: number) => frames.delete(id)),
  );
});

afterEach(() => {
  mounted.forEach((wrapper) => wrapper.unmount());
  mounted.clear();
  tooltipService.hide();
  globalThis.ResizeObserver = originalResizeObserver;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('v-overflow-tooltip', () => {
  it.each([
    { client: 100, scroll: 80 },
    { client: 100, scroll: 100 },
    { client: 0, scroll: 250 },
  ])('does not show a tooltip for untruncated or hidden text (%j)', (size) => {
    defaultDimensions = size;
    const { el } = mountText();
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value).toBeNull();
    el.dispatchEvent(new Event('focusin'));
    expect(tooltipService.active.value).toBeNull();
    expect(el.hasAttribute('title')).toBe(false);
    expect(el.classList.contains('rp-overflow-tooltip')).toBe(true);
  });

  it('shows the complete trimmed text only when its horizontal content overflows', () => {
    const { el } = mountText({ class: 'existing-text' });
    expect(tooltipService.active.value).toBeNull();
    expect(el.hasAttribute('title')).toBe(false);
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value).toEqual({
      target: el,
      text: '系统完成站点 doudui.cc 的自动探测。',
    });
    expect(el.hasAttribute('title')).toBe(false);
    expect(el.classList.contains('existing-text')).toBe(true);
    expect(el.classList.contains('rp-overflow-tooltip')).toBe(true);
  });

  it('updates the bound tooltip and falls back to the latest DOM text without stale content', async () => {
    const { el, text, tooltip, className } = mountText({
      text: '原始概要',
      tooltip: '指定的完整内容',
    });
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value?.text).toBe('指定的完整内容');

    tooltip.value = '更新后的指定内容';
    await nextTick();
    expect(tooltipService.active.value?.text).toBe('更新后的指定内容');

    tooltip.value = '';
    await nextTick();
    expect(tooltipService.active.value).toBeNull();
    expect(el.hasAttribute('title')).toBe(false);

    tooltip.value = undefined;
    text.value = '  更新后的概要  ';
    className.value = 'updated-text';
    await nextTick();
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value?.text).toBe('更新后的概要');
    expect(el.classList.contains('updated-text')).toBe(true);
    expect(el.classList.contains('rp-overflow-tooltip')).toBe(true);

    text.value = ' \n ';
    await nextTick();
    expect(tooltipService.active.value).toBeNull();
    expect(el.hasAttribute('title')).toBe(false);
  });

  it('reacts to resizing and DOM text changes while combining observer work into one frame', () => {
    const { el } = mountText();
    expect(resizeObservers[0].observe).toHaveBeenCalledWith(el);
    expect(mutationObservers[0].observe).toHaveBeenCalledWith(el, expect.any(Object));
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value?.target).toBe(el);

    dimensions.set(el, { client: 300, scroll: 250 });
    resizeObservers[0].emit();
    mutationObservers[0].emit();
    resizeObservers[0].emit();
    expect(frames.size).toBe(1);
    flushFrame();
    expect(tooltipService.active.value).toBeNull();

    dimensions.set(el, { client: 100, scroll: 250 });
    resizeObservers[0].emit();
    flushFrame();
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value?.text).toBe('系统完成站点 doudui.cc 的自动探测。');

    el.textContent = 'DOM 外部更新后的概要';
    mutationObservers[0].emit();
    flushFrame();
    expect(tooltipService.active.value?.text).toBe('DOM 外部更新后的概要');
  });

  it('rechecks actual overflow on hover and focus even before an observer has fired', async () => {
    const { el, wrapper } = mountText();
    dimensions.set(el, { client: 300, scroll: 250 });
    await wrapper.trigger('mouseenter');
    expect(tooltipService.active.value).toBeNull();

    dimensions.set(el, { client: 100, scroll: 250 });
    el.textContent = '焦点进入时的完整内容';
    await wrapper.trigger('focusin');
    expect(tooltipService.active.value?.text).toBe('焦点进入时的完整内容');
    expect(el.hasAttribute('title')).toBe(false);
  });

  it('keeps independent hosts isolated when one is resized or removed', async () => {
    const first = mountText({ text: '第一条概要' });
    const second = mountText({ text: '第二条概要' });
    await first.wrapper.trigger('mouseenter');
    expect(tooltipService.active.value?.target).toBe(first.el);
    await second.wrapper.trigger('mouseenter');
    expect(tooltipService.active.value).toEqual({ target: second.el, text: '第二条概要' });
    dimensions.set(first.el, { client: 300, scroll: 250 });
    resizeObservers[0].emit();
    flushFrame();
    await first.wrapper.trigger('mouseleave');
    expect(tooltipService.active.value).toEqual({ target: second.el, text: '第二条概要' });

    unmount(first.wrapper);
    expect(tooltipService.active.value?.target).toBe(second.el);
    second.el.textContent = '第二条更新后的概要';
    await second.wrapper.trigger('mouseenter');
    expect(tooltipService.active.value?.text).toBe('第二条更新后的概要');
    expect(resizeObservers[1].disconnect).not.toHaveBeenCalled();
  });

  it('keeps the tooltip while hover or focus remains and closes it when both leave', () => {
    const { el } = mountText();
    el.dispatchEvent(new Event('mouseenter'));
    el.dispatchEvent(new Event('focusin'));
    el.dispatchEvent(new Event('mouseleave'));
    expect(tooltipService.active.value?.target).toBe(el);
    el.dispatchEvent(new Event('focusout'));
    expect(tooltipService.active.value).toBeNull();

    el.dispatchEvent(new Event('mouseenter'));
    el.dispatchEvent(new Event('focusin'));
    el.dispatchEvent(new Event('focusout'));
    expect(tooltipService.active.value?.target).toBe(el);
    el.dispatchEvent(new Event('mouseleave'));
    expect(tooltipService.active.value).toBeNull();
  });

  it('does not open an inactive tooltip in response to content or observer updates', async () => {
    const { el, text } = mountText();
    text.value = '更新但没有被悬浮的内容';
    await nextTick();
    resizeObservers[0].emit();
    mutationObservers[0].emit();
    flushFrame();
    expect(tooltipService.active.value).toBeNull();
    expect(el.hasAttribute('title')).toBe(false);
  });

  it('releases observers, listeners and pending work, then restores the original attributes', () => {
    const { el, wrapper } = mountText({
      title: '原有提示',
      class: 'existing-text rp-overflow-tooltip',
    });
    const removeListener = vi.spyOn(el, 'removeEventListener');
    expect(el.hasAttribute('title')).toBe(false);
    el.dispatchEvent(new Event('mouseenter'));
    expect(tooltipService.active.value?.target).toBe(el);
    resizeObservers[0].emit();
    const [[pendingId, pendingCallback]] = [...frames.entries()];

    unmount(wrapper);
    expect(resizeObservers[0].disconnect).toHaveBeenCalledOnce();
    expect(mutationObservers[0].disconnect).toHaveBeenCalledOnce();
    expect(removeListener.mock.calls.map(([event]) => event)).toEqual(
      expect.arrayContaining(['mouseenter', 'mouseleave', 'focusin', 'focusout']),
    );
    expect(cancelAnimationFrame).toHaveBeenCalledWith(pendingId);
    expect(frames.size).toBe(0);
    expect(tooltipService.active.value).toBeNull();
    expect(el.title).toBe('原有提示');
    expect(el.classList.contains('existing-text')).toBe(true);
    expect(el.classList.contains('rp-overflow-tooltip')).toBe(true);

    el.title = '卸载后由其他代码接管';
    const requestCount = vi.mocked(requestAnimationFrame).mock.calls.length;
    resizeObservers[0].emit();
    mutationObservers[0].emit();
    pendingCallback(0);
    el.dispatchEvent(new Event('mouseenter'));
    el.dispatchEvent(new Event('focusin'));
    expect(el.title).toBe('卸载后由其他代码接管');
    expect(tooltipService.active.value).toBeNull();
    expect(vi.mocked(requestAnimationFrame).mock.calls.length).toBe(requestCount);
    expect(frames.size).toBe(0);
  });

  it('leaves no owned attributes or queued work after repeated mounts and unmounts', () => {
    for (let index = 0; index < 5; index++) {
      const { el, wrapper } = mountText({ class: 'existing-text' });
      el.dispatchEvent(new Event('mouseenter'));
      expect(tooltipService.active.value?.target).toBe(el);
      resizeObservers[index].emit();
      unmount(wrapper);
      expect(el.hasAttribute('title')).toBe(false);
      expect(el.classList.contains('rp-overflow-tooltip')).toBe(false);
      expect(el.classList.contains('existing-text')).toBe(true);
      expect(frames.size).toBe(0);
      expect(tooltipService.active.value).toBeNull();
    }
    expect(resizeObservers).toHaveLength(5);
    expect(mutationObservers).toHaveLength(5);
    expect(resizeObservers.every((observer) => observer.disconnect.mock.calls.length === 1)).toBe(
      true,
    );
    expect(mutationObservers.every((observer) => observer.disconnect.mock.calls.length === 1)).toBe(
      true,
    );
  });
});
