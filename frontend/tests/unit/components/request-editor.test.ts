import type { RequestParameterDraft } from '@/types/request-parameters';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Tabs } from 'antdv-next';
import { afterEach, expect, it } from 'vitest';
import { computed, defineComponent, ref } from 'vue';

import { buildRequestHeaders } from '@/utils/request-headers';
import { buildRequestParametersPreview, createRequestParameters } from '@/utils/request-parameters';
import { siteRequestPreview } from '@/views/config/sites/site-request';

import RequestEditor from '@/components/request-parameter/request-editor.vue';

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
});

it('Headers 在首位，与 Body 使用相同 JSON 弹窗且导入互不影响，预览同步更新', async () => {
  const headers = ref<RequestParameterDraft[]>([]);
  const parameters = ref(createRequestParameters({ temperature: 0.2 }));
  const preview = computed(() =>
    siteRequestPreview(
      'https://example.test/usagev1/res',
      'openai-compatible',
      headers.value,
      buildRequestParametersPreview(parameters.value, ''),
    ),
  );
  const wrapper = mount(
    defineComponent({
      components: { RequestEditor },
      setup: () => ({ headers, parameters, preview }),
      template:
        '<RequestEditor v-model:headers="headers" v-model:parameters="parameters" :preview="preview" />',
    }),
    { attachTo: document.body, global: { plugins: [AntdvNext] } },
  );
  dispose = () => wrapper.unmount();
  expect(wrapper.getComponent(Tabs).props('activeKey')).toBe('headers');
  expect(
    wrapper
      .getComponent(Tabs)
      .props('items')!
      .map((item: { key: string }) => item.key),
  ).toEqual(['headers', 'body', 'preview']);
  async function paste(section: string, json: string) {
    await wrapper
      .get(`[data-request-section="${section}"]`)
      .findAll('button')
      .find((button) => button.text().replace(/\s/gu, '') === '粘贴JSON')!
      .trigger('click');
    await flushPromises();
    const modal = new DOMWrapper(document.querySelector('.ant-modal')!);
    await modal.get('[aria-label="JSON 参数"]').setValue(json);
    await modal.get('.ant-modal-footer .ant-btn-primary').trigger('click');
    await flushPromises();
  }
  await paste('headers', '{"kc":"model_sy","x.route":"visible"}');
  expect(buildRequestHeaders(headers.value)).toEqual([
    { key: 'kc', value: 'model_sy' },
    { key: 'x.route', value: 'visible' },
  ]);
  expect(buildRequestParametersPreview(parameters.value, '')).toEqual({ temperature: 0.2 });
  await paste('body', '{"reasoning":{"effort":"high"}}');
  expect(buildRequestHeaders(headers.value)).toEqual([
    { key: 'kc', value: 'model_sy' },
    { key: 'x.route', value: 'visible' },
  ]);
  await wrapper
    .findAll('[role="tab"]')
    .find((tab) => tab.text() === '请求预览')!
    .trigger('click');
  await flushPromises();
  const rendered = wrapper.get('[aria-label="有效请求预览"]');
  expect(rendered.findAll('h4').map((heading) => heading.text())).toEqual([
    '请求地址',
    '请求头 Headers',
    '请求体 Body',
  ]);
  expect(rendered.get('[aria-label="请求地址"] pre').text()).toBe(
    'POST https://example.test/usagev1/res',
  );
  expect(JSON.parse(rendered.get('[aria-label="请求头 JSON"]').text())).toEqual({
    kc: 'model_sy',
    'x.route': 'visible',
    authorization: '[REDACTED]',
    'content-type': 'application/json',
    accept: '*/*',
    'user-agent': '[应用默认 User-Agent]',
  });
  expect(rendered.get('[aria-label="请求头 Headers"]').findAll('pre')).toHaveLength(1);
  expect(rendered.text()).not.toContain('用户配置');
  expect(rendered.text()).not.toContain('程序默认 · 凭据已脱敏');
  expect(JSON.parse(rendered.get('[aria-label="请求体 JSON"]').text())).toEqual({
    temperature: 0.2,
    reasoning: { effort: 'high' },
  });
  expect(buildRequestParametersPreview(parameters.value, '')).toEqual({
    temperature: 0.2,
    reasoning: { effort: 'high' },
  });
});
