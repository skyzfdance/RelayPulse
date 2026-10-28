/**
 * 全局提示单例：message / notification / modal 的唯一收口。
 *
 * - 队列语义：同一时刻只展示一条提示，其余 FIFO 排队，关闭后自动出队；
 *   排队期间内容完全相同的提示会被去重，避免连环弹窗。
 * - 渲染实例由 FeedbackHost 从 <a-app> 的 App.useApp() 注入（带主题上下文），
 *   注入前入队的提示会在注入后统一放出，因此 store / api 层可在组件外调用。
 * - 约定：业务代码一律经由 useFeedback() 或 callBackend 的 feedback 选项
 *   触发提示，禁止直接引用 antdv 的提示实例/静态方法。
 */

export type FeedbackMode = 'message' | 'modal' | 'notification';
export type FeedbackKind = 'success' | 'error' | 'info' | 'warning';

/** 只声明队列用到的能力，便于单测注入替身。 */
export interface FeedbackInstances {
  message: Record<
    FeedbackKind,
    (config: { content: string; duration?: number; onClose?: () => void }) => unknown
  >;
  notification: Record<
    FeedbackKind,
    (config: {
      message: string;
      description?: string;
      duration?: number;
      onClose?: () => void;
    }) => unknown
  >;
  modal: Record<
    FeedbackKind,
    (config: {
      title: string;
      content: string;
      okText?: string;
      onOk?: () => void;
      onCancel?: () => void;
    }) => unknown
  >;
}

interface FeedbackItem {
  mode: FeedbackMode;
  kind: FeedbackKind;
  content: string;
}

const FEEDBACK_TITLES: Record<FeedbackKind, string> = {
  success: '操作成功',
  error: '操作失败',
  info: '提示',
  warning: '注意',
};

const queue: FeedbackItem[] = [];
let current: FeedbackItem | null = null;
let instances: FeedbackInstances | null = null;

/** 由 FeedbackHost 在挂载时调用：注入 <a-app> 上下文实例并放出积压提示。 */
export function bindFeedbackInstances(next: FeedbackInstances): void {
  instances = next;
  drain();
}

function isSameItem(a: FeedbackItem, b: FeedbackItem): boolean {
  return a.mode === b.mode && a.kind === b.kind && a.content === b.content;
}

function enqueue(item: FeedbackItem): void {
  if (current && isSameItem(current, item)) return;
  if (queue.some((pending) => isSameItem(pending, item))) return;
  queue.push(item);
  drain();
}

function drain(): void {
  if (!instances || current) return;
  const item = queue.shift();
  if (!item) return;
  current = item;
  const done = () => {
    current = null;
    drain();
  };
  if (item.mode === 'message') {
    instances.message[item.kind]({ content: item.content, onClose: done });
  } else if (item.mode === 'notification') {
    instances.notification[item.kind]({
      message: FEEDBACK_TITLES[item.kind],
      description: item.content,
      onClose: done,
    });
  } else {
    instances.modal[item.kind]({
      title: FEEDBACK_TITLES[item.kind],
      content: item.content,
      okText: '知道了',
      onOk: done,
      onCancel: done,
    });
  }
}

/** 直连提示的默认模式：错误走 modal（与 callBackend 失败默认一致），其余走 message。 */
const DEFAULT_MODE_BY_KIND: Record<FeedbackKind, FeedbackMode> = {
  success: 'message',
  info: 'message',
  warning: 'message',
  error: 'modal',
};

export function useFeedback() {
  return {
    success: (content: string, mode: FeedbackMode = DEFAULT_MODE_BY_KIND.success) =>
      enqueue({ mode, kind: 'success', content }),
    error: (content: string, mode: FeedbackMode = DEFAULT_MODE_BY_KIND.error) =>
      enqueue({ mode, kind: 'error', content }),
    info: (content: string, mode: FeedbackMode = DEFAULT_MODE_BY_KIND.info) =>
      enqueue({ mode, kind: 'info', content }),
    warning: (content: string, mode: FeedbackMode = DEFAULT_MODE_BY_KIND.warning) =>
      enqueue({ mode, kind: 'warning', content }),
  };
}

/** 仅供单测复位使用。 */
export function resetFeedbackForTests(): void {
  queue.length = 0;
  current = null;
  instances = null;
}
