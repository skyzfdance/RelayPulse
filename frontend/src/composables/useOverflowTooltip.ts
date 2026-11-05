import { shallowReadonly, shallowRef } from 'vue';

interface OverflowTooltipTarget {
  target: HTMLElement;
  text: string;
}

// 只有当前显示的提示持有 DOM 引用；隐藏或宿主卸载时立即释放。
const active = shallowRef<OverflowTooltipTarget | null>(null);

function show(target: HTMLElement, text: string) {
  active.value = { target, text };
}

function hide(target?: HTMLElement) {
  if (!target || active.value?.target === target) active.value = null;
}

export function useOverflowTooltip() {
  return { active: shallowReadonly(active), show, hide };
}
