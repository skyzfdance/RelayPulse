import type { MaybeRefOrGetter } from 'vue';

import { onScopeDispose, ref, shallowRef, toValue, watch } from 'vue';

/** 按需读取与旧响应隔离；错误含义、文案、重试规则和审计由调用方决定。 */
export function useLazyResource<K, T>(
  key: MaybeRefOrGetter<K>,
  active: MaybeRefOrGetter<boolean>,
  fetcher: (key: K) => Promise<T>,
) {
  const content = shallowRef<T>();
  const failure = shallowRef<{ cause: unknown }>();
  const loading = ref(false);
  let revision = 0;
  let disposed = false;

  function current() {
    const captured = revision;
    return () => !disposed && captured === revision;
  }
  function reset() {
    revision += 1;
    content.value = undefined;
    failure.value = undefined;
    loading.value = false;
  }
  async function reload() {
    if (disposed || loading.value) return;
    reset();
    const isCurrent = current();
    loading.value = true;
    try {
      const result = await fetcher(toValue(key));
      if (isCurrent()) content.value = result;
    } catch (cause) {
      if (isCurrent()) failure.value = { cause };
    } finally {
      if (isCurrent()) loading.value = false;
    }
  }
  watch(() => toValue(key), reset, { flush: 'sync' });
  watch(
    () => [toValue(key), toValue(active)] as const,
    ([, open]) => {
      if (open && content.value === undefined && !failure.value && !loading.value) void reload();
    },
    { immediate: true },
  );
  onScopeDispose(() => {
    disposed = true;
    revision += 1;
  });
  return { content, loading, failure, reload, current };
}
