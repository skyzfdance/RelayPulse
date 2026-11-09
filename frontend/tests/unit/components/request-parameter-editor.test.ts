import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Button, Checkbox, Input, Modal, Select } from 'antdv-next';
import { afterEach, describe, expect, it } from 'vitest';
import { reactive } from 'vue';

import { buildRequestParametersPreview, createRequestParameters } from '@/utils/request-parameters';

import RequestParameterEditor from '@/components/request-parameter/editor.vue';

const cleanup: (() => void)[] = [];
afterEach(() => {
  cleanup.splice(0).forEach((dispose) => dispose());
});

function mountEditor(body: Record<string, unknown>, disabled = false) {
  const parameters = reactive(createRequestParameters(body));
  const wrapper = mount(RequestParameterEditor, {
    attachTo: document.body,
    props: { parameters, disabled, modelName: 'gpt-demo' },
    global: { plugins: [AntdvNext] },
  });
  cleanup.push(() => wrapper.unmount());
  return { wrapper, parameters };
}

type EditorPage = ReturnType<typeof mountEditor>;
function importButton(page: EditorPage) {
  return page.wrapper
    .findAllComponents(Button)
    .find((button) => button.text().replace(/\s/gu, '') === '粘贴JSON')!;
}
async function openImport(page: EditorPage) {
  await importButton(page).trigger('click');
  await flushPromises();
  return new DOMWrapper(document.querySelector('.ant-modal')!);
}

describe('公共请求参数编辑器', () => {
  it('粘贴 JSON 自动识别参数类型，同名参数覆盖手动值，其余参数保留', async () => {
    const page = mountEditor({
      temperature: 0.2,
      seed: 42,
      reasoning: { effort: 'high', summary: 'auto' },
    });
    page.parameters[0]!.enabled = false;
    const modal = await openImport(page);
    expect(modal.text()).toContain('粘贴 JSON 参数');
    await modal.get('[aria-label="JSON 参数"]').setValue(
      JSON.stringify({
        temperature: 0.8,
        stream: true,
        max_tokens: 32,
        reasoning: { effort: 'low' },
        metadata: [{ id: 1 }],
        stop: null,
      }),
    );
    await modal.get('.ant-modal-footer .ant-btn-primary').trigger('click');
    await flushPromises();

    expect(
      page.wrapper.findComponent(Modal).exists() && page.wrapper.getComponent(Modal).props('open'),
    ).toBeFalsy();
    expect(buildRequestParametersPreview(page.parameters, '')).toEqual({
      temperature: 0.8,
      seed: 42,
      stream: true,
      max_tokens: 32,
      reasoning: { effort: 'low', summary: 'auto' },
      metadata: [{ id: 1 }],
      stop: null,
    });
    expect(page.parameters.filter((parameter) => parameter.key === 'temperature')).toHaveLength(1);
    expect(page.parameters.map(({ key, type, enabled }) => ({ key, type, enabled }))).toEqual(
      expect.arrayContaining([
        { key: 'temperature', type: 'number', enabled: true },
        { key: 'stream', type: 'boolean', enabled: true },
        { key: 'max_tokens', type: 'number', enabled: true },
        { key: 'reasoning', type: 'object', enabled: true },
        { key: 'reasoning.effort', type: 'string', enabled: true },
        { key: 'metadata', type: 'json', enabled: true },
        { key: 'stop', type: 'json', enabled: true },
      ]),
    );
  });

  it('无效 JSON 保留弹窗及原参数，取消后重新打开清空输入和错误', async () => {
    const page = mountEditor({ temperature: 0.2, stream: true });
    const before = JSON.stringify(page.parameters);
    const modal = await openImport(page);
    for (const value of ['{"temperature":', '[]']) {
      await modal.get('[aria-label="JSON 参数"]').setValue(value);
      await modal.get('.ant-modal-footer .ant-btn-primary').trigger('click');
      await flushPromises();
      expect(modal.get('[role="alert"]').text()).toContain('JSON');
      expect(page.wrapper.getComponent(Modal).props('open')).toBe(true);
      expect(JSON.stringify(page.parameters)).toBe(before);
    }

    await modal
      .findAll('.ant-modal-footer button')
      .find((button) => button.text().replace(/\s/gu, '') === '取消')!
      .trigger('click');
    await flushPromises();
    expect(JSON.stringify(page.parameters)).toBe(before);

    const reopened = await openImport(page);
    expect(reopened.get<HTMLTextAreaElement>('[aria-label="JSON 参数"]').element.value).toBe('');
    expect(reopened.find('[role="alert"]').exists()).toBe(false);
  });

  it('禁用时不能强制打开导入，弹窗开启后禁用也阻止强制解析', async () => {
    const page = mountEditor({ temperature: 0.2 }, true);
    const before = JSON.stringify(page.parameters);
    expect(importButton(page).props('disabled')).toBe(true);
    importButton(page).vm.$emit('click');
    await flushPromises();
    expect(
      page.wrapper.findComponent(Modal).exists() && page.wrapper.getComponent(Modal).props('open'),
    ).toBeFalsy();

    await page.wrapper.setProps({ disabled: false });
    const modal = await openImport(page);
    await modal.get('[aria-label="JSON 参数"]').setValue('{"temperature":0.8}');
    await page.wrapper.setProps({ disabled: true });
    expect(modal.get('[aria-label="JSON 参数"]').attributes('disabled')).toBeDefined();
    expect(page.wrapper.getComponent(Modal).props('okButtonProps')).toMatchObject({
      disabled: true,
    });
    page.wrapper.getComponent(Modal).vm.$emit('ok');
    await flushPromises();
    expect(JSON.stringify(page.parameters)).toBe(before);
    expect(page.wrapper.getComponent(Modal).props('open')).toBe(true);

    await page.wrapper.setProps({ disabled: false });
    await modal.get('.ant-modal-footer .ant-btn-primary').trigger('click');
    await flushPromises();
    expect(buildRequestParametersPreview(page.parameters, '')).toEqual({ temperature: 0.8 });
  });

  it('父级改名保留子字段，停用和恢复父级保留子级开关与输入', async () => {
    const { wrapper, parameters } = mountEditor({
      reasoning: { effort: 'high', summary: 'auto' },
    });
    const row = (key: string) => wrapper.get(`[data-parameter-key="${key}"]`);
    await row('reasoning.summary').get('input[type="checkbox"]').setValue(false);
    await row('reasoning').get('[aria-label^="参数字段 "]').setValue('options');
    expect(parameters.map((parameter) => parameter.key)).toEqual([
      'options',
      'options.effort',
      'options.summary',
    ]);
    expect(buildRequestParametersPreview(parameters, '')).toEqual({
      options: { effort: 'high' },
    });

    // 即使绕过下拉选项发事件，有子级的父级仍须保持 object。
    row('options').getComponent(Select).vm.$emit('update:value', 'string');
    expect(parameters[0]!.type).toBe('object');
    await row('options').get('input[type="checkbox"]').setValue(false);
    const before = JSON.stringify(parameters);
    const child = row('options.effort');
    expect(child.get<HTMLInputElement>('input[type="checkbox"]').element.checked).toBe(true);
    expect(child.get<HTMLInputElement>('input[type="checkbox"]').element.disabled).toBe(true);
    expect(child.get('[aria-label^="参数字段 "]').attributes('disabled')).toBeDefined();
    for (const input of child.findAllComponents(Input)) input.vm.$emit('update:value', 'blocked');
    for (const select of child.findAllComponents(Select)) select.vm.$emit('update:value', 'number');
    child.getComponent(Checkbox).vm.$emit('update:checked', false);
    child.getComponent(Button).vm.$emit('click');
    await flushPromises();
    expect(JSON.stringify(parameters)).toBe(before);
    expect(buildRequestParametersPreview(parameters, '')).toEqual({});

    await row('options').get('input[type="checkbox"]').setValue(true);
    expect(buildRequestParametersPreview(parameters, '')).toEqual({
      options: { effort: 'high' },
    });
    expect(parameters.find((parameter) => parameter.key === 'options.summary')).toMatchObject({
      enabled: false,
      value: 'auto',
    });
  });

  it('模型绑定展示当前名称且不可改字段与类型，仍可停用或删除', async () => {
    const { wrapper, parameters } = mountEditor({ model: '', temperature: 0.2 });
    const row = () => wrapper.get('[data-parameter-key="model"]');
    expect(row().text()).toContain('gpt-demo');
    expect(row().get('[aria-label^="参数字段 "]').attributes('disabled')).toBeDefined();
    expect(row().find('[aria-label^="参数值 "]').exists()).toBe(false);
    expect(row().getComponent(Select).props('disabled')).toBe(true);
    row().getComponent(Input).vm.$emit('update:value', 'prompt');
    row().getComponent(Select).vm.$emit('update:value', 'json');
    await flushPromises();
    expect(parameters[0]).toMatchObject({
      key: 'model',
      type: 'string',
      value: '',
      binding: 'model',
    });
    await wrapper.setProps({ modelName: 'gpt-next' });
    expect(row().text()).toContain('gpt-next');
    expect(buildRequestParametersPreview(parameters, 'gpt-next').model).toBe('gpt-next');
    await row().get('input[type="checkbox"]').setValue(false);
    expect(buildRequestParametersPreview(parameters, 'gpt-next')).not.toHaveProperty('model');
    await row().get('input[type="checkbox"]').setValue(true);
    await row().get('button[title="删除参数"]').trigger('click');
    expect(wrapper.find('[data-parameter-key="model"]').exists()).toBe(false);
    expect(buildRequestParametersPreview(parameters, 'gpt-next')).toEqual({ temperature: 0.2 });
  });

  it('整体禁用时阻止所有控件的强制事件修改草稿，解锁后正常编辑', async () => {
    const { wrapper, parameters } = mountEditor(
      {
        model: '',
        reasoning: { effort: 'high' },
        stream: true,
        messages: [{ role: 'user', content: '只回复 OK' }],
        max_tokens: 8,
      },
      true,
    );
    const before = JSON.stringify(parameters);
    const controls = [
      ...wrapper.findAllComponents(Input),
      ...wrapper.findAllComponents(Input.TextArea),
      ...wrapper.findAllComponents(Select),
      ...wrapper.findAllComponents(Checkbox),
      ...wrapper.findAllComponents(Button),
    ];
    expect(controls.length).toBeGreaterThan(20);
    for (const control of controls) expect(control.props().disabled).toBe(true);
    for (const input of wrapper.findAllComponents(Input)) input.vm.$emit('update:value', 'blocked');
    for (const input of wrapper.findAllComponents(Input.TextArea))
      input.vm.$emit('update:value', '[]');
    for (const select of wrapper.findAllComponents(Select))
      select.vm.$emit('update:value', 'number');
    for (const checkbox of wrapper.findAllComponents(Checkbox))
      checkbox.vm.$emit('update:checked', false);
    for (const button of wrapper.findAllComponents(Button)) button.vm.$emit('click');
    await flushPromises();
    expect(JSON.stringify(parameters)).toBe(before);

    await wrapper.setProps({ disabled: false });
    await wrapper
      .get('[data-parameter-key="reasoning.effort"] [aria-label^="参数值 "]')
      .setValue('low');
    expect(buildRequestParametersPreview(parameters, 'gpt-demo').reasoning).toEqual({
      effort: 'low',
    });
  });

  it('业务校验错误可由调用方提供，清除后显示公共提示', async () => {
    const { wrapper } = mountEditor({});
    await wrapper.setProps({ error: '题面、model、stream 等系统字段不可覆盖' });
    expect(wrapper.get('[role="alert"]').text()).toBe('题面、model、stream 等系统字段不可覆盖');
    await wrapper.setProps({ error: undefined });
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('支持对象路径；关闭父级时子参数不发送，空对象仍可发送。');
  });
});
