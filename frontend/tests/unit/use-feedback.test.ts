import { beforeEach, describe, expect, it } from 'vitest';

import {
  type FeedbackInstances,
  type FeedbackKind,
  type FeedbackMode,
  bindFeedbackInstances,
  resetFeedbackForTests,
  useFeedback,
} from '@/composables/useFeedback';

interface ShownItem {
  mode: FeedbackMode;
  kind: FeedbackKind;
  payload: Record<string, unknown>;
  close: () => void;
}

function createFakeInstances() {
  const shown: ShownItem[] = [];
  const kinds: FeedbackKind[] = ['success', 'error', 'info', 'warning'];
  const make = (mode: FeedbackMode) =>
    Object.fromEntries(
      kinds.map((kind) => [
        kind,
        (payload: Record<string, unknown>) => {
          const item: ShownItem = {
            mode,
            kind,
            payload,
            close: () => {
              const index = shown.indexOf(item);
              if (index >= 0) shown.splice(index, 1);
              const callback =
                mode === 'modal'
                  ? (payload.onOk as (() => void) | undefined)
                  : (payload.onClose as (() => void) | undefined);
              callback?.();
            },
          };
          shown.push(item);
        },
      ]),
    );
  const instances = {
    message: make('message'),
    notification: make('notification'),
    modal: make('modal'),
  } as unknown as FeedbackInstances;
  return { shown, instances };
}

let fake: ReturnType<typeof createFakeInstances>;

beforeEach(() => {
  resetFeedbackForTests();
  fake = createFakeInstances();
});

describe('全局提示单例队列', () => {
  it('实例注入前入队的提示，注入后统一放出', () => {
    useFeedback().error('后端不可用');
    expect(fake.shown).toHaveLength(0);

    bindFeedbackInstances(fake.instances);
    expect(fake.shown).toHaveLength(1);
    expect(fake.shown[0].payload.content).toBe('后端不可用');
  });

  it('同一时刻只展示一条，关闭后按 FIFO 出队', () => {
    bindFeedbackInstances(fake.instances);
    const feedback = useFeedback();
    feedback.error('第一个');
    feedback.error('第二个');
    feedback.error('第三个');

    expect(fake.shown.map((item) => item.payload.content)).toEqual(['第一个']);
    fake.shown[0].close();
    expect(fake.shown.map((item) => item.payload.content)).toEqual(['第二个']);
    fake.shown[0].close();
    expect(fake.shown.map((item) => item.payload.content)).toEqual(['第三个']);
  });

  it('排队期间内容完全相同的提示被去重', () => {
    bindFeedbackInstances(fake.instances);
    const feedback = useFeedback();
    feedback.error('相同错误');
    feedback.error('相同错误');
    feedback.error('不同错误');

    fake.shown[0].close();
    expect(fake.shown.map((item) => item.payload.content)).toEqual(['不同错误']);
  });

  it('直连默认模式：success→message，error→modal（带标题与按钮文案）', () => {
    bindFeedbackInstances(fake.instances);
    const feedback = useFeedback();
    feedback.success('已保存');
    feedback.error('已失败');

    expect(fake.shown[0]).toMatchObject({ mode: 'message', kind: 'success' });
    // 关闭当前 message 后，排队中的 error modal 出队顶上（fake 关闭会移除当前项，故仍取 [0]）
    fake.shown[0].close();
    expect(fake.shown[0]).toMatchObject({ mode: 'modal', kind: 'error' });
    expect(fake.shown[0].payload.title).toBe('操作失败');
    expect(fake.shown[0].payload.okText).toBe('知道了');
  });

  it('notification 模式映射标题与描述', () => {
    bindFeedbackInstances(fake.instances);
    useFeedback().error('网络中断', 'notification');

    expect(fake.shown[0].payload).toMatchObject({
      message: '操作失败',
      description: '网络中断',
    });
  });
});
