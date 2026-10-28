import type { Options } from 'sortablejs';
import type { Ref } from 'vue';

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';
import { defineComponent, nextTick, ref } from 'vue';

import { useSortable } from '@/composables/useSortable';
const mock = vi.hoisted(() => ({
  options: undefined as Options | undefined,
  node: undefined as HTMLElement | undefined,
  destroy: vi.fn(),
  option: vi.fn(),
}));
vi.mock('sortablejs', () => ({
  default: {
    create: (node: HTMLElement, options: Options) => {
      mock.node = node;
      mock.options = options;
      return {
        destroy: mock.destroy,
        option: mock.option,
        toArray: () => [...node.children].map((c) => c.getAttribute('data-sort-id')!),
        sort: (ids: string[]) =>
          ids.forEach((id) => node.appendChild(node.querySelector(`[data-sort-id="${id}"]`)!)),
      };
    },
  },
}));
function harness(reorder: (ids: string[]) => void, disabled: Ref<boolean> = ref(false)) {
  return mount(
    defineComponent({
      setup() {
        const node = ref<HTMLElement | null>(null);
        useSortable(node, { disabled, onReorder: reorder });
        return { node };
      },
      template:
        '<div ref="node"><div data-sort-id="a"/><div data-sort-id="b"/><div data-sort-id="c"/></div>',
    }),
  );
}
it('拖拽后先恢复 DOM；业务拒绝保存时顺序不变；卸载销毁实例', async () => {
  const reorder = vi.fn();
  const wrapper = harness(reorder);
  await nextTick();
  mock.options!.onStart?.({} as never);
  mock.node!.appendChild(mock.node!.children[0]);
  mock.options!.onEnd?.({ oldIndex: 0, newIndex: 2 } as never);
  expect(reorder).toHaveBeenCalledWith(['b', 'c', 'a']);
  expect([...mock.node!.children].map((c) => c.getAttribute('data-sort-id'))).toEqual([
    'a',
    'b',
    'c',
  ]);
  wrapper.unmount();
  expect(mock.destroy).toHaveBeenCalled();
});
describe('拖拽提交锁', () => {
  it('响应 disabled 变化，没有移动时不发送保存请求', async () => {
    const disabled = ref(false);
    const reorder = vi.fn();
    const wrapper = harness(reorder, disabled);
    await nextTick();
    disabled.value = true;
    await nextTick();
    expect(mock.option).toHaveBeenCalledWith('disabled', true);
    mock.options!.onStart?.({} as never);
    mock.options!.onEnd?.({ oldIndex: 0, newIndex: 0 } as never);
    expect(reorder).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
