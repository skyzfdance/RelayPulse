import type { ObjectDirective } from 'vue';

import { useOverflowTooltip } from '@/composables/useOverflowTooltip';

type OverflowTooltipValue = string | null | undefined;

interface OverflowTooltipController {
  update(value: OverflowTooltipValue): void;
  destroy(): void;
}

const className = 'rp-overflow-tooltip';
const controllers = new WeakMap<HTMLElement, OverflowTooltipController>();

function createController(
  el: HTMLElement,
  initialValue: OverflowTooltipValue,
): OverflowTooltipController {
  const originalTitle = el.getAttribute('title');
  const hadClass = el.classList.contains(className);
  let value = initialValue;
  let disposed = false;
  let frame: number | undefined;
  let hovered = false;
  let focused = false;
  const { active, show, hide } = useOverflowTooltip();

  el.classList.add(className);

  function overflowingText() {
    // 绑定期间不允许浏览器原生 title 与 antdv Tooltip 同时出现。
    el.removeAttribute('title');
    const text = (value ?? el.textContent ?? '').trim();
    return text && el.clientWidth > 0 && el.scrollWidth > el.clientWidth ? text : null;
  }

  function measure() {
    if (disposed) return;
    const text = overflowingText();
    if (!text) hide(el);
    else if (active.value?.target === el) show(el, text);
  }

  function reveal() {
    if (disposed) return;
    const text = overflowingText();
    if (text) show(el, text);
    else hide(el);
  }

  function mouseenter() {
    hovered = true;
    reveal();
  }

  function mouseleave() {
    hovered = false;
    if (!focused) hide(el);
  }

  function focusin() {
    focused = true;
    reveal();
  }

  function focusout(event: FocusEvent) {
    if (event.relatedTarget instanceof Node && el.contains(event.relatedTarget)) return;
    focused = false;
    if (!hovered) hide(el);
  }

  function schedule() {
    if (disposed || frame !== undefined) return;
    frame = window.requestAnimationFrame(() => {
      frame = undefined;
      measure();
    });
  }

  const resizeObserver = new ResizeObserver(schedule);
  const mutationObserver = new MutationObserver(schedule);
  resizeObserver.observe(el);
  // 不监听属性，避免设置 title/class 再次触发自身测量。
  mutationObserver.observe(el, { childList: true, characterData: true, subtree: true });
  el.addEventListener('mouseenter', mouseenter);
  el.addEventListener('mouseleave', mouseleave);
  el.addEventListener('focusin', focusin);
  el.addEventListener('focusout', focusout);
  measure();

  return {
    update(nextValue) {
      value = nextValue;
      el.classList.add(className);
      measure();
    },
    destroy() {
      disposed = true;
      resizeObserver.disconnect();
      mutationObserver.disconnect();
      el.removeEventListener('mouseenter', mouseenter);
      el.removeEventListener('mouseleave', mouseleave);
      el.removeEventListener('focusin', focusin);
      el.removeEventListener('focusout', focusout);
      if (frame !== undefined) window.cancelAnimationFrame(frame);
      frame = undefined;
      hide(el);
      if (originalTitle === null) el.removeAttribute('title');
      else el.setAttribute('title', originalTitle);
      if (!hadClass) el.classList.remove(className);
    },
  };
}

/**
 * 文本容器单行省略，只有实际溢出时才通过全局 antdv Tooltip 宿主显示全文。
 * `v-overflow-tooltip` 默认读取 textContent，也可绑定字符串作为提示内容。
 * 宿主需有受约束的宽度；绑定期间 title 由指令管理，不要同时绑定 title。
 */
export const vOverflowTooltip: ObjectDirective<HTMLElement, OverflowTooltipValue> = {
  mounted(el, binding) {
    controllers.set(el, createController(el, binding.value));
  },
  updated(el, binding) {
    controllers.get(el)?.update(binding.value);
  },
  beforeUnmount(el) {
    controllers.get(el)?.destroy();
    controllers.delete(el);
  },
};
