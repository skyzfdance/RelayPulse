import type { VueWrapper } from '@vue/test-utils';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { ConfigProvider, theme } from 'antdv-next';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, ref, withDirectives } from 'vue';

import { useOverflowTooltip } from '@/composables/useOverflowTooltip';
import { vOverflowTooltip } from '@/directives/overflow-tooltip';

import OverflowTooltipHost from '@/components/OverflowTooltipHost.vue';

const mounted = new Set<VueWrapper>();
const tooltipService = useOverflowTooltip();

beforeEach(() => tooltipService.hide());
afterEach(async () => {
  mounted.forEach((wrapper) => wrapper.unmount());
  mounted.clear();
  tooltipService.hide();
  await flushPromises();
  vi.restoreAllMocks();
});

function mountTooltip() {
  const text = ref('系统完成站点 doudui.cc 的自动探测。');
  const targetVisible = ref(true);
  const hostVisible = ref(true);
  const Root = defineComponent({
    setup: () => () =>
      h(
        ConfigProvider,
        {
          theme: {
            cssVar: { key: 'overflow-tooltip-integration-theme' },
            algorithm: theme.darkAlgorithm,
            token: { colorBgSpotlight: '#123456', motion: false },
          },
        },
        {
          default: () =>
            h('section', { 'data-testid': 'scroll-container' }, [
              targetVisible.value
                ? withDirectives(
                    h('span', { 'data-testid': 'summary', title: '旧的原生提示' }, text.value),
                    [[vOverflowTooltip]],
                  )
                : null,
              hostVisible.value ? h(OverflowTooltipHost) : null,
            ]),
        },
      ),
  });
  const wrapper = mount(Root, { attachTo: document.body, global: { plugins: [AntdvNext] } });
  mounted.add(wrapper);
  const target = wrapper.get('[data-testid="summary"]');
  const el = target.element as HTMLElement;
  const size = { client: 120, scroll: 360 };
  Object.defineProperties(el, {
    clientWidth: { configurable: true, get: () => size.client },
    scrollWidth: { configurable: true, get: () => size.scroll },
  });
  vi.spyOn(el, 'getBoundingClientRect').mockReturnValue(new DOMRect(24, 48, 120, 24));
  return { wrapper, target, el, text, size, targetVisible, hostVisible };
}

async function expectVisible(text: string) {
  await flushPromises();
  await vi.waitFor(() => {
    expect(document.querySelector('[role="tooltip"]')?.textContent).toBe(text);
  });
}

async function expectHidden() {
  await flushPromises();
  expect(document.querySelector('[role="tooltip"]')).toBeNull();
  expect(document.querySelector('.rp-overflow-tooltip-anchor')).toBeNull();
  expect(tooltipService.active.value).toBeNull();
}

describe('antd overflow tooltip integration', () => {
  it('shows the real themed tooltip only for overflow without moving the original DOM', async () => {
    const { wrapper, target, el, text } = mountTooltip();
    const originalParent = el.parentElement;
    expect(el.hasAttribute('title')).toBe(false);
    await expectHidden();

    await target.trigger('mouseenter');
    await expectVisible(text.value);

    const popup = document.querySelector<HTMLElement>('.ant-tooltip')!;
    expect(popup.classList.contains('overflow-tooltip-integration-theme')).toBe(true);
    expect(getComputedStyle(popup).getPropertyValue('--ant-color-bg-spotlight').trim()).toBe(
      '#123456',
    );
    const anchor = document.querySelector<HTMLElement>('.rp-overflow-tooltip-anchor')!;
    expect(anchor.style.position).toBe('fixed');
    expect(anchor.style.left).toBe('24px');
    expect(anchor.style.top).toBe('48px');
    expect(anchor.style.pointerEvents).toBe('none');
    expect(el.parentElement).toBe(originalParent);
    expect(wrapper.get('[data-testid="summary"]').element).toBe(el);
    expect(el.hasAttribute('title')).toBe(false);

    await target.trigger('mouseleave');
    await expectHidden();
  });

  it('does not render a tooltip when the complete text fits or the target is hidden', async () => {
    const { target, el, size } = mountTooltip();
    for (const nextSize of [
      { client: 360, scroll: 360 },
      { client: 400, scroll: 360 },
      { client: 0, scroll: 360 },
    ]) {
      Object.assign(size, nextSize);
      await target.trigger('mouseenter');
      await target.trigger('focusin');
      await expectHidden();
      expect(el.hasAttribute('title')).toBe(false);
    }
  });

  it('refreshes visible content and closes when an update no longer overflows', async () => {
    const { target, el, text, size } = mountTooltip();
    await target.trigger('mouseenter');
    await expectVisible(text.value);

    text.value = '用户修改了站点 ai.input.im 的连接地址和访问凭据。';
    await nextTick();
    await expectVisible(text.value);
    expect(el.hasAttribute('title')).toBe(false);
    expect(document.querySelectorAll('[role="tooltip"]')).toHaveLength(1);

    size.scroll = 80;
    text.value = '短概要';
    await nextTick();
    await expectHidden();
  });

  it('dismisses on nested scrolling, Escape, resizing and window blur', async () => {
    const { wrapper, target, text } = mountTooltip();
    const dismissals = [
      () =>
        wrapper
          .get('[data-testid="scroll-container"]')
          .element.dispatchEvent(new Event('scroll', { bubbles: false })),
      () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })),
      () => window.dispatchEvent(new Event('resize')),
      () => window.dispatchEvent(new Event('blur')),
    ];
    for (const dismiss of dismissals) {
      await target.trigger('mouseenter');
      await expectVisible(text.value);
      dismiss();
      await expectHidden();
    }
  });

  it('cleans up the active target and portal when the target or host is removed', async () => {
    const { target, text, targetVisible, hostVisible, wrapper } = mountTooltip();
    await target.trigger('mouseenter');
    await expectVisible(text.value);
    targetVisible.value = false;
    await nextTick();
    await expectHidden();

    targetVisible.value = true;
    await nextTick();
    const replacement = wrapper.get('[data-testid="summary"]');
    Object.defineProperties(replacement.element, {
      clientWidth: { configurable: true, value: 120 },
      scrollWidth: { configurable: true, value: 360 },
    });
    await replacement.trigger('mouseenter');
    await expectVisible(text.value);
    hostVisible.value = false;
    await nextTick();
    await expectHidden();
  });

  it('releases all host window listeners and its portal on unmount', async () => {
    const addListener = vi.spyOn(window, 'addEventListener');
    const removeListener = vi.spyOn(window, 'removeEventListener');
    const { wrapper, target, text } = mountTooltip();
    await target.trigger('mouseenter');
    await expectVisible(text.value);

    const scrollRegistration = addListener.mock.calls.find(
      ([event, , capture]) => event === 'scroll' && capture === true,
    );
    expect(scrollRegistration).toBeDefined();
    const dismiss = scrollRegistration![1];
    expect(addListener).toHaveBeenCalledWith('resize', dismiss);
    expect(addListener).toHaveBeenCalledWith('blur', dismiss);
    const keydownHandlers = addListener.mock.calls
      .filter(([event]) => event === 'keydown')
      .map(([, callback]) => callback);
    expect(keydownHandlers.length).toBeGreaterThan(0);

    wrapper.unmount();
    mounted.delete(wrapper);
    await expectHidden();
    expect(removeListener).toHaveBeenCalledWith('scroll', dismiss, true);
    expect(removeListener).toHaveBeenCalledWith('resize', dismiss);
    expect(removeListener).toHaveBeenCalledWith('blur', dismiss);
    for (const keydown of keydownHandlers) {
      expect(
        removeListener.mock.calls.some(
          ([event, callback]) => event === 'keydown' && callback === keydown,
        ),
      ).toBe(true);
    }
  });
});
