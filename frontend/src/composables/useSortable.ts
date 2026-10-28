import type { MaybeRefOrGetter, Ref } from 'vue';

import Sortable from 'sortablejs';
import { onScopeDispose, toValue, watch } from 'vue';
/** Sortable 只采集顺序。立即恢复原 DOM，再由 Vue 根据业务保存结果更新列表。 */
export function useSortable(
  element: Ref<HTMLElement | null>,
  options: {
    disabled?: MaybeRefOrGetter<boolean>;
    onReorder: (ids: string[]) => void;
  },
) {
  let sortable: Sortable | undefined;
  let original: string[] = [];
  watch(
    element,
    (node) => {
      sortable?.destroy();
      sortable = undefined;
      if (!node) return;
      sortable = Sortable.create(node, {
        animation: 150,
        handle: '[data-sort-handle]',
        draggable: '[data-sort-id]',
        dataIdAttr: 'data-sort-id',
        disabled: toValue(options.disabled) ?? false,
        onStart: () => {
          original = sortable?.toArray() ?? [];
        },
        onEnd: ({ oldIndex, newIndex }) => {
          if (!sortable) return;
          const next = sortable.toArray();
          sortable.sort(original, false);
          if (oldIndex !== newIndex && next.length === original.length) options.onReorder(next);
        },
      });
    },
    { flush: 'post', immediate: true },
  );
  watch(
    () => toValue(options.disabled),
    (value) => sortable?.option('disabled', Boolean(value)),
  );
  onScopeDispose(() => sortable?.destroy());
}
