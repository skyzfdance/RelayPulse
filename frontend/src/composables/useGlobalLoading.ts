/**
 * 页面级全局 loading 单例。
 *
 * - 引用计数：并发多个请求共享一个遮罩（由 GlobalLoadingHost 渲染），
 *   全部结束后才隐藏。
 * - 防闪烁：首个请求起 SHOW_DELAY_MS 内完成的短任务不出现遮罩；
 *   已出现的遮罩最短停留 MIN_DISPLAY_MS，避免尾部闪现。
 */
import { readonly, ref } from 'vue';

const SHOW_DELAY_MS = 200;
const MIN_DISPLAY_MS = 300;

/** 全局 loading 默认文案（callBackend 的 loadingTips 不传时用它）。 */
export const DEFAULT_LOADING_TIP = '数据加载中...';

const visible = ref(false);
const tip = ref<string>(DEFAULT_LOADING_TIP);
let activeCount = 0;
let showTimer: ReturnType<typeof setTimeout> | null = null;
let hideTimer: ReturnType<typeof setTimeout> | null = null;
let shownAt = 0;

export interface GlobalLoadingGuard {
  release: () => void;
}

function acquireGlobalLoading(nextTip: string = DEFAULT_LOADING_TIP): GlobalLoadingGuard {
  // 动态文案：全局只有一个遮罩，后进入的请求覆盖当前文案（最后设置者可见）
  tip.value = nextTip;
  activeCount += 1;
  // 新请求抵消待执行的隐藏，避免遮罩在连续请求间闪断
  if (hideTimer !== null) {
    clearTimeout(hideTimer);
    hideTimer = null;
  }
  if (activeCount === 1 && !visible.value && showTimer === null) {
    showTimer = setTimeout(() => {
      showTimer = null;
      shownAt = Date.now();
      visible.value = true;
    }, SHOW_DELAY_MS);
  }
  return {
    release: () => {
      activeCount = Math.max(0, activeCount - 1);
      if (activeCount > 0) return;
      if (showTimer !== null) {
        // 从未显示过：直接取消，全程无感
        clearTimeout(showTimer);
        showTimer = null;
      } else if (visible.value && hideTimer === null) {
        const remaining = Math.max(0, MIN_DISPLAY_MS - (Date.now() - shownAt));
        hideTimer = setTimeout(() => {
          hideTimer = null;
          visible.value = false;
        }, remaining);
      }
    },
  };
}

/** 供宿主组件读取展示状态与文案；业务代码不应直接消费。 */
export function useGlobalLoading() {
  return { visible: readonly(visible), tip: readonly(tip) };
}

/** 仅供单测复位使用。 */
export function resetGlobalLoadingForTests(): void {
  if (showTimer !== null) clearTimeout(showTimer);
  if (hideTimer !== null) clearTimeout(hideTimer);
  activeCount = 0;
  showTimer = null;
  hideTimer = null;
  shownAt = 0;
  visible.value = false;
  tip.value = DEFAULT_LOADING_TIP;
}

export { acquireGlobalLoading };
