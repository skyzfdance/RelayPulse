import { flushPromises } from '@vue/test-utils';
import { expect, it } from 'vitest';
import { effectScope, ref } from 'vue';

import { useLazyResource } from '@/composables/useLazyResource';

import { deferred } from '../../support/deferred';

it('切换记录、重新读取或卸载后隔离旧响应；收起展开复用已经取得的内容', async () => {
  const first = deferred<string>();
  const second = deferred<string>();
  const third = deferred<string>();
  const key = ref('first');
  const open = ref(false);
  const scope = effectScope();
  const resource = scope.run(() =>
    useLazyResource(key, open, (id) => {
      return { first, second, third }[id]!.promise;
    }),
  )!;
  expect(resource.loading.value).toBe(false);
  open.value = true;
  await flushPromises();
  key.value = 'second';
  await flushPromises();
  first.resolve('stale');
  second.resolve('current');
  await flushPromises();
  expect(resource.content.value).toBe('current');
  open.value = false;
  await flushPromises();
  open.value = true;
  await flushPromises();
  expect(resource.content.value).toBe('current');
  const isCurrent = resource.current();
  key.value = 'third';
  await flushPromises();
  expect(isCurrent()).toBe(false);
  scope.stop();
  third.resolve('after disposal');
  await flushPromises();
  expect(resource.content.value).toBeUndefined();
});
