import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Drawer, Input } from 'antdv-next';
import { afterEach, describe, expect, it, vi } from 'vitest';

import IqQuestionDrawer from '@/views/iq-test/components/IqQuestionDrawer.vue';

const dispose: (() => void)[] = [];
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
  vi.restoreAllMocks();
});

async function openDrawer(props: Partial<InstanceType<typeof IqQuestionDrawer>['$props']> = {}) {
  const wrapper = mount(IqQuestionDrawer, {
    attachTo: document.body,
    props,
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  await flushPromises();
  const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
  const save = async () => {
    await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
    await flushPromises();
  };
  return { wrapper, drawer, save };
}

describe('IQ 题目编辑抽屉', () => {
  it('空白题目无法提交，展示中文表单校验', async () => {
    const { wrapper, drawer, save } = await openDrawer();
    await save();
    expect(drawer.text()).toContain('请输入题目内容');
    expect(wrapper.emitted('save')).toBeUndefined();
    await drawer.get('textarea[aria-label="题目内容"]').setValue('   ');
    await save();
    expect(drawer.text()).toContain('请输入题目内容');
    expect(wrapper.emitted('save')).toBeUndefined();
  });

  it('最大长度由表单校验保护，4096 字题目可提交且空答案转换为空值', async () => {
    const { wrapper, drawer, save } = await openDrawer();
    const input = wrapper.findAllComponents(Input.TextArea)[0]!;
    input.vm.$emit('update:value', '题'.repeat(4097));
    await save();
    expect(drawer.text()).toContain('题目最多 4096 个字符');
    expect(wrapper.emitted('save')).toBeUndefined();
    input.vm.$emit('update:value', '题'.repeat(4096));
    await save();
    expect(wrapper.emitted('save')).toEqual([
      [{ content: '题'.repeat(4096), referenceAnswer: null }],
    ]);
  });

  it('编辑从现有题目建草稿，取消不改变源数据，保存失败保留输入', async () => {
    const original = { id: 'q-existing', content: '原题目', referenceAnswer: '原答案' };
    const { wrapper, drawer, save } = await openDrawer({ question: original });
    expect(drawer.text()).toContain('编辑题目');
    await drawer.get('textarea[aria-label="题目内容"]').setValue('  修改后的题目  ');
    await drawer.get('textarea[aria-label="参考答案"]').setValue('新答案');
    await save();
    expect(wrapper.emitted('save')).toEqual([
      [{ content: '修改后的题目', referenceAnswer: '新答案' }],
    ]);
    await wrapper.setProps({ error: '保存请求失败' });
    expect(drawer.text()).toContain('保存请求失败');
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="题目内容"]').element.value).toBe(
      '  修改后的题目  ',
    );
    await drawer.get('.ant-drawer-footer .ant-btn:not(.ant-btn-primary)').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(original).toEqual({ id: 'q-existing', content: '原题目', referenceAnswer: '原答案' });
  });

  it('保存中禁用输入、取消及重复保存，关闭事件不会绕过保护', async () => {
    const { wrapper, drawer, save } = await openDrawer({ saving: true });
    expect(drawer.get('textarea').attributes('disabled')).toBeDefined();
    await save();
    wrapper.getComponent(Drawer).vm.$emit('close');
    wrapper.findAllComponents(Input.TextArea)[0]!.vm.$emit('update:value', '不应写入');
    await flushPromises();
    expect(drawer.get<HTMLTextAreaElement>('textarea').element.value).toBe('');
    expect(wrapper.emitted('save')).toBeUndefined();
    expect(wrapper.emitted('close')).toBeUndefined();
  });
});

describe('IQ 题目只读抽屉', () => {
  it('只读模式展示完整多行题面和参考答案，安全显示特殊文本，无编辑控件', async () => {
    const content = '第一段问题\n第二段问题\n<script>alert("文本")</script>';
    const answer = '第一步解答\n第二步解释';
    const { wrapper, drawer } = await openDrawer({
      question: { id: 'internal-question-id', content, referenceAnswer: answer },
      mode: 'view',
    });
    expect(drawer.text()).toContain('查看题目');
    expect(drawer.get('[aria-label="题目全文"] > div').element.textContent?.trim()).toBe(content);
    expect(drawer.get('[aria-label="题目参考答案"] > div').element.textContent?.trim()).toBe(
      answer,
    );
    expect(drawer.find('script').exists()).toBe(false);
    expect(drawer.find('textarea').exists()).toBe(false);
    expect(drawer.find('form').exists()).toBe(false);
    expect(drawer.find('.ant-drawer-footer').exists()).toBe(false);
    expect(drawer.text()).not.toContain('user prompt');
    expect(drawer.text()).not.toContain('internal-question-id');
    await drawer.get('[aria-label="关闭题目抽屉"]').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(wrapper.emitted('save')).toBeUndefined();
  });

  it('参考答案为空时显示明确空态，写入状态参数可省略', async () => {
    const { wrapper, drawer } = await openDrawer({
      mode: 'view',
      question: { id: 'no-answer', content: '无答案题目', referenceAnswer: null },
    });
    expect(drawer.get('[aria-label="题目参考答案"]').text()).toContain('暂无参考答案');
    expect(wrapper.emitted('save')).toBeUndefined();
  });
});
