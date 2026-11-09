import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Button, Input, Pagination, Select, Switch, Table } from 'antdv-next';
import { createPinia, disposePinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as bankApi from '@/api/iq-test/bank';
import { bindFeedbackInstances, resetFeedbackForTests } from '@/composables/useFeedback';
import { seedIqState } from '@/mocks/handlers/iq-test';
import { useIqTestStore } from '@/stores/iq-test';

import IqBankView from '@/views/iq-test/bank/IqBankView.vue';

const cleanup: (() => void)[] = [];
beforeEach(() => {
  window.history.replaceState({}, '', '/iq-test/bank');
  const pinia = createPinia();
  setActivePinia(pinia);
  cleanup.push(() => disposePinia(pinia));
  seedIqState(true);
  resetFeedbackForTests();
});
afterEach(async () => {
  cleanup
    .reverse()
    .splice(0)
    .forEach((fn) => fn());
  await flushPromises();
  vi.restoreAllMocks();
});

async function mountBank() {
  const wrapper = mount(IqBankView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  cleanup.push(() => wrapper.unmount());
  await useIqTestStore().connect();
  await useIqTestStore().refresh();
  await flushPromises();
  return wrapper;
}

async function newQuestion(wrapper: Awaited<ReturnType<typeof mountBank>>) {
  await wrapper
    .findAllComponents(Button)
    .find((item) => item.text().includes('新建题目'))!
    .trigger('click');
  await flushPromises();
  return new DOMWrapper(document.querySelector('.ant-drawer')!);
}

async function save(drawer: DOMWrapper<Element>) {
  await drawer.get('.ant-drawer-footer .ant-btn-primary').trigger('click');
  await flushPromises();
}

describe('IQ 题库浏览器 mock 交互', () => {
  it('完成新增、编辑、启停、筛选与删除确认的完整旅程', async () => {
    const warnings = vi.spyOn(console, 'warn');
    const remove = vi.spyOn(bankApi, 'deleteIqQuestion');
    const wrapper = await mountBank();
    const store = useIqTestStore();
    expect(wrapper.text()).toContain('还没有测试题目');
    expect(wrapper.find('button[aria-label="刷新题库"]').exists()).toBe(false);

    const drawer = await newQuestion(wrapper);
    await drawer.get('textarea[aria-label="题目内容"]').setValue('为什么天空是蓝色的？');
    await drawer.get('textarea[aria-label="参考答案"]').setValue('瑞利散射');
    await save(drawer);
    await vi.waitFor(() => expect(store.questions).toHaveLength(1));
    expect(store.questions[0]).toMatchObject({
      content: '为什么天空是蓝色的？',
      referenceAnswer: '瑞利散射',
      enabled: true,
    });
    expect(wrapper.text()).toContain('有参考答案');

    await wrapper.get('button[aria-label="编辑题目 为什么天空是蓝色的？"]').trigger('click');
    await flushPromises();
    const editor = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await editor.get('textarea[aria-label="题目内容"]').setValue('为什么日落偏红？');
    await save(editor);
    await vi.waitFor(() => expect(wrapper.text()).toContain('为什么日落偏红？'));
    expect(wrapper.text()).not.toContain('为什么天空是蓝色的？');

    wrapper.getComponent(Switch).vm.$emit('change', false);
    await flushPromises();
    expect(store.questions[0]?.enabled).toBe(false);
    wrapper.getComponent(Select).vm.$emit('update:value', 'enabled');
    await flushPromises();
    expect(wrapper.text()).toContain('没有符合筛选条件的题目');
    wrapper.getComponent(Select).vm.$emit('update:value', 'disabled');
    await flushPromises();
    expect(wrapper.text()).toContain('为什么日落偏红？');
    expect(wrapper.get('input[aria-label="搜索题目内容"]').attributes('type')).toBe('search');

    const questionId = store.questions[0]!.id;
    await wrapper.get('button[aria-label="删除题目 为什么日落偏红？"]').trigger('click');
    await flushPromises();
    const confirmation = new DOMWrapper(document.querySelector('.ant-popconfirm')!);
    expect(confirmation.text()).toContain('历史运行保存的题目快照不受影响');
    expect(remove).not.toHaveBeenCalled();
    await confirmation.get('.ant-popconfirm-buttons .ant-btn-primary').trigger('click');
    await flushPromises();
    expect(remove).toHaveBeenCalledWith(questionId);
    expect(store.questions).toHaveLength(0);
    expect(
      warnings.mock.calls.filter(([message]) => String(message).includes('[Vue warn]')),
    ).toEqual([]);
  });

  it('保存失败留在抽屉并保留输入，重新保存成功后关闭', async () => {
    const globalError = vi.fn();
    const feedbackHandlers = {
      success: vi.fn(),
      error: globalError,
      info: vi.fn(),
      warning: vi.fn(),
    };
    bindFeedbackInstances({
      message: feedbackHandlers,
      notification: feedbackHandlers,
      modal: feedbackHandlers,
    });
    const wrapper = await mountBank();
    const drawer = await newQuestion(wrapper);
    await drawer.get('textarea[aria-label="题目内容"]').setValue('保存失败也应保留的问题');
    window.history.replaceState({}, '', '/iq-test/bank?iqScenario=action-error');
    await save(drawer);
    await vi.waitFor(() => expect(drawer.text()).toContain('保存失败'));
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="题目内容"]').element.value).toBe(
      '保存失败也应保留的问题',
    );
    expect(useIqTestStore().questions).toHaveLength(0);
    expect(globalError).not.toHaveBeenCalled();
    window.history.replaceState({}, '', '/iq-test/bank');
    await save(drawer);
    await vi.waitFor(() => expect(useIqTestStore().questions).toHaveLength(1));
    expect(wrapper.findComponent(IqBankView).exists()).toBe(true);
    expect(document.querySelector('.ant-drawer')).toBeNull();
  });

  it('读取失败明确提示且可重试，不以空数据伪装成功', async () => {
    window.history.replaceState({}, '', '/iq-test/bank?iqScenario=error');
    const wrapper = await mountBank();
    expect(wrapper.text()).toContain('题库读取失败');
    expect(wrapper.text()).toContain('暂时无法读取或保存 IQ 数据');
    window.history.replaceState({}, '', '/iq-test/bank');
    await wrapper.get('.ant-alert-actions button').trigger('click');
    await flushPromises();
    expect(wrapper.text()).not.toContain('题库读取失败');
    expect(wrapper.text()).toContain('还没有测试题目');
  });
});

describe('IQ 题库题面查看与分页', () => {
  async function populate(count: number) {
    for (let index = 1; index <= count; index++) {
      await bankApi.saveIqQuestion({
        content: `测试题目 ${index}\n第二行保留原文。`,
        referenceAnswer: index % 2 ? `参考答案 ${index}\n继续说明。` : null,
        enabled: index <= 11,
      });
    }
  }

  it('三列表格默认每页10条，页码与每页数量只改变展示，分页位于独立滚动区外', async () => {
    await populate(21);
    const wrapper = await mountBank();
    const store = useIqTestStore();
    const viewport = wrapper.get('[aria-label="IQ 题库列表"]');
    const pagination = wrapper.getComponent(Pagination);
    const table = wrapper.getComponent(Table);
    expect(table.props('columns')?.map((column) => column.title)).toEqual(['题目', '启用', '操作']);
    expect(table.props('pagination')).toBe(false);
    expect(pagination.props('pageSize')).toBe(10);
    expect(pagination.props('total')).toBe(21);
    expect(viewport.find('.ant-pagination').exists()).toBe(false);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    expect(wrapper.text()).toContain('测试题目 21');
    expect(wrapper.text()).not.toContain('测试题目 11');
    expect(store.questions).toHaveLength(21);

    viewport.element.scrollTop = 240;
    pagination.vm.$emit('change', 2, 10);
    await flushPromises();
    expect(viewport.element.scrollTop).toBe(0);
    expect(pagination.props('current')).toBe(2);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    expect(wrapper.text()).toContain('测试题目 11');
    expect(wrapper.text()).not.toContain('测试题目 21');
    expect(store.questions).toHaveLength(21);

    pagination.vm.$emit('change', 3, 10);
    await flushPromises();
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(1);
    wrapper.getComponent(Select).vm.$emit('update:value', 'enabled');
    await flushPromises();
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('total')).toBe(11);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
    pagination.vm.$emit('change', 2, 10);
    await flushPromises();
    pagination.vm.$emit('change', 2, 20);
    await flushPromises();
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('pageSize')).toBe(20);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(11);
    expect(store.questions).toHaveLength(21);
  });

  it('删除末页最后一题后回到仍有效的上一页', async () => {
    await populate(11);
    const wrapper = await mountBank();
    const pagination = wrapper.getComponent(Pagination);
    pagination.vm.$emit('change', 2, 10);
    await flushPromises();
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(1);
    await wrapper.get('button[aria-label^="删除题目"]').trigger('click');
    await flushPromises();
    await new DOMWrapper(
      document.querySelector('.ant-popconfirm-buttons .ant-btn-primary')!,
    ).trigger('click');
    await vi.waitFor(() => expect(useIqTestStore().questions).toHaveLength(10));
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('total')).toBe(10);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(10);
  });

  it('即时检索全量题面而非当前页，忽略大小写与首尾空格，改变和清空检索保留每页数量', async () => {
    const content = '第一行问题\n第二行说明\n第三行 NEEDLE 中文关键字 [a+b].* 保留完整题面';
    await populate(21);
    await bankApi.saveIqQuestion({ content, referenceAnswer: '只在答案出现的词', enabled: true });
    const wrapper = await mountBank();
    const store = useIqTestStore();
    const pagination = wrapper.getComponent(Pagination);
    const search = wrapper.get<HTMLInputElement>('input[aria-label="搜索题目内容"]');
    expect(wrapper.getComponent(Input).props('allowClear')).toBe(true);
    expect(search.attributes('type')).toBe('search');
    pagination.vm.$emit('change', 1, 20);
    await flushPromises();
    pagination.vm.$emit('change', 2, 20);
    await flushPromises();
    expect(pagination.props('current')).toBe(2);
    expect(wrapper.text()).not.toContain('NEEDLE 中文关键字');

    await search.setValue('  nEeDlE 中文  ');
    await flushPromises();
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('pageSize')).toBe(20);
    expect(pagination.props('total')).toBe(1);
    expect(wrapper.findAll('.question-preview').map((item) => item.text())).toEqual([content]);
    expect(wrapper.text()).toContain('第 1–1 条，共 1 条');
    expect(store.questions).toHaveLength(22);

    await search.setValue('[a+b].*');
    await flushPromises();
    expect(pagination.props('total')).toBe(1);
    await search.setValue('只在答案出现的词');
    await flushPromises();
    expect(pagination.props('total')).toBe(0);
    expect(wrapper.text()).toContain('没有匹配的题目');
    await wrapper.get('.ant-input-clear-icon').trigger('click');
    await flushPromises();
    expect(search.element.value).toBe('');
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('pageSize')).toBe(20);
    expect(pagination.props('total')).toBe(22);

    await search.setValue('测试题目');
    await flushPromises();
    expect(pagination.props('total')).toBe(21);
    pagination.vm.$emit('change', 2, 20);
    await flushPromises();
    expect(pagination.props('current')).toBe(2);
    await wrapper.get('.ant-input-clear-icon').trigger('click');
    await flushPromises();
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('pageSize')).toBe(20);
    expect(pagination.props('total')).toBe(22);

    pagination.vm.$emit('change', 2, 20);
    await flushPromises();
    await search.setValue('   ');
    await flushPromises();
    expect(pagination.props('current')).toBe(1);
    expect(pagination.props('pageSize')).toBe(20);
    expect(pagination.props('total')).toBe(22);
    expect(wrapper.findAll('tbody tr[data-row-key]')).toHaveLength(20);
  });

  it('关键词与启停状态取交集，无匹配可清空搜索保留状态或恢复全部题目', async () => {
    await bankApi.saveIqQuestion({
      content: 'Alpha中文 启用题',
      referenceAnswer: null,
      enabled: true,
    });
    await bankApi.saveIqQuestion({
      content: 'alpha中文 停用题',
      referenceAnswer: null,
      enabled: false,
    });
    await bankApi.saveIqQuestion({
      content: '其他启用题',
      referenceAnswer: 'Alpha中文',
      enabled: true,
    });
    const wrapper = await mountBank();
    const pagination = wrapper.getComponent(Pagination);
    const search = wrapper.get<HTMLInputElement>('input[aria-label="搜索题目内容"]');
    const status = wrapper.getComponent(Select);
    await search.setValue(' ALPHA中文 ');
    await flushPromises();
    expect(pagination.props('total')).toBe(2);
    status.vm.$emit('update:value', 'enabled');
    await flushPromises();
    expect(pagination.props('total')).toBe(1);
    expect(wrapper.findAll('.question-preview').map((item) => item.text())).toEqual([
      'Alpha中文 启用题',
    ]);
    status.vm.$emit('update:value', 'disabled');
    await flushPromises();
    expect(pagination.props('total')).toBe(1);
    expect(wrapper.findAll('.question-preview').map((item) => item.text())).toEqual([
      'alpha中文 停用题',
    ]);

    await search.setValue('不存在的关键词');
    await flushPromises();
    expect(wrapper.text()).toContain('没有匹配的题目');
    expect(wrapper.text()).not.toContain('还没有测试题目');
    expect(wrapper.text()).not.toContain('没有符合筛选条件的题目');
    expect(pagination.props('total')).toBe(0);
    await wrapper
      .findAllComponents(Button)
      .find((item) => item.text() === '清空搜索')!
      .trigger('click');
    await flushPromises();
    expect(search.element.value).toBe('');
    expect(status.props('value')).toBe('disabled');
    expect(pagination.props('total')).toBe(1);

    await search.setValue('再次无匹配');
    await flushPromises();
    await wrapper
      .findAllComponents(Button)
      .find((item) => item.text() === '查看全部题目')!
      .trigger('click');
    await flushPromises();
    expect(search.element.value).toBe('');
    expect(status.props('value')).toBe('all');
    expect(pagination.props('total')).toBe(3);
    expect(useIqTestStore().questions).toHaveLength(3);
  });

  it('空库保持新建入口，状态筛选空态与关键词无匹配空态明确区分', async () => {
    const wrapper = await mountBank();
    const search = wrapper.get('input[aria-label="搜索题目内容"]');
    await search.setValue('空库关键词');
    await flushPromises();
    expect(wrapper.text()).toContain('还没有测试题目');
    expect(wrapper.text()).not.toContain('没有匹配的题目');
    expect(wrapper.findAllComponents(Button).some((item) => item.text() === '新建第一道题目')).toBe(
      true,
    );

    await bankApi.saveIqQuestion({ content: '唯一启用题', referenceAnswer: null, enabled: true });
    await useIqTestStore().refresh();
    await search.setValue('   ');
    wrapper.getComponent(Select).vm.$emit('update:value', 'disabled');
    await flushPromises();
    expect(wrapper.text()).toContain('没有符合筛选条件的题目');
    expect(wrapper.text()).not.toContain('没有匹配的题目');
    expect(wrapper.text()).not.toContain('还没有测试题目');
    await wrapper
      .findAllComponents(Button)
      .find((item) => item.text() === '查看全部题目')!
      .trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('唯一启用题');
    expect(wrapper.getComponent(Pagination).props('total')).toBe(1);
  });

  it('题面和参考答案入口都打开同一个只读抽屉，关闭后可继续编辑', async () => {
    const content = '第一行问题\n第二行说明\n第三行保留完整题面<script>示例文本</script>';
    const answer = '第一步答案\n第二步解释';
    await bankApi.saveIqQuestion({ content, referenceAnswer: answer, enabled: true });
    const wrapper = await mountBank();
    const preview = wrapper.get('.question-preview');
    expect(preview.element.textContent?.trim()).toBe(content);
    expect(preview.find('script').exists()).toBe(false);
    await preview.trigger('click');
    await flushPromises();
    let drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(document.querySelectorAll('.ant-drawer')).toHaveLength(1);
    expect(drawer.get('[aria-label="题目全文"]').text()).toContain(content);
    expect(drawer.get('[aria-label="题目参考答案"]').text()).toContain(answer);
    expect(drawer.find('textarea').exists()).toBe(false);
    expect(drawer.find('.ant-drawer-footer').exists()).toBe(false);
    await drawer.get('[aria-label="关闭题目抽屉"]').trigger('click');
    await flushPromises();
    await wrapper.get('button[aria-label^="查看参考答案"]').trigger('click');
    await flushPromises();
    drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(drawer.get('[aria-label="题目参考答案"]').text()).toContain(answer);
    await drawer.get('[aria-label="关闭题目抽屉"]').trigger('click');
    await flushPromises();
    await wrapper.get('button[aria-label^="编辑题目"]').trigger('click');
    await flushPromises();
    drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(drawer.text()).toContain('编辑题目');
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="题目内容"]').element.value).toBe(
      content,
    );
    expect(drawer.find('.ant-drawer-footer .ant-btn-primary').exists()).toBe(true);
  });

  it('查看按题目ID跟随刷新更新全文和答案，题目删除后关闭', async () => {
    const question = await bankApi.saveIqQuestion({
      content: '刷新前的题目',
      referenceAnswer: '刷新前的答案',
      enabled: true,
    });
    const wrapper = await mountBank();
    const store = useIqTestStore();
    await wrapper.get('.question-preview').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(drawer.get('[aria-label="题目全文"]').text()).toContain('刷新前的题目');

    await bankApi.saveIqQuestion(
      { content: '刷新后的题目\n保留完整换行', referenceAnswer: '刷新后的答案', enabled: true },
      question.id,
    );
    await store.refresh();
    await flushPromises();
    expect(drawer.get('[aria-label="题目全文"]').text()).toContain('刷新后的题目\n保留完整换行');
    expect(drawer.get('[aria-label="题目参考答案"]').text()).toContain('刷新后的答案');
    expect(drawer.text()).not.toContain('刷新前的');
    expect(document.querySelectorAll('.ant-drawer')).toHaveLength(1);

    await bankApi.deleteIqQuestion(question.id);
    await store.refresh();
    await flushPromises();
    expect(store.questions).toHaveLength(0);
    expect(document.querySelector('.ant-drawer')).toBeNull();
  });

  it('后台刷新保留题面与答案草稿，保存沿用最新启停状态', async () => {
    const question = await bankApi.saveIqQuestion({
      content: '编辑开始时的题目',
      referenceAnswer: '编辑开始时的答案',
      enabled: true,
    });
    const wrapper = await mountBank();
    await wrapper.get('button[aria-label^="编辑题目"]').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('textarea[aria-label="题目内容"]').setValue('尚未保存的题目草稿');
    await drawer.get('textarea[aria-label="参考答案"]').setValue('尚未保存的答案草稿');

    await bankApi.saveIqQuestion(
      { content: '后台更新的题目', referenceAnswer: '后台更新的答案', enabled: false },
      question.id,
    );
    await useIqTestStore().refresh();
    await flushPromises();
    expect(wrapper.text()).toContain('后台更新的题目');
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="题目内容"]').element.value).toBe(
      '尚未保存的题目草稿',
    );
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="参考答案"]').element.value).toBe(
      '尚未保存的答案草稿',
    );
    await save(drawer);
    expect(useIqTestStore().questions[0]).toMatchObject({
      id: question.id,
      content: '尚未保存的题目草稿',
      referenceAnswer: '尚未保存的答案草稿',
      enabled: false,
    });
    expect(document.querySelector('.ant-drawer')).toBeNull();
  });

  it('编辑中的题目被删除后保存报错并保留草稿，不会新建题目', async () => {
    const question = await bankApi.saveIqQuestion({
      content: '编辑后会被删除的题目',
      referenceAnswer: null,
      enabled: true,
    });
    const wrapper = await mountBank();
    await wrapper.get('button[aria-label^="编辑题目"]').trigger('click');
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('textarea[aria-label="题目内容"]').setValue('删除后仍保留的草稿');
    await bankApi.deleteIqQuestion(question.id);
    await useIqTestStore().refresh();
    const saveRequest = vi.spyOn(bankApi, 'saveIqQuestion');

    await save(drawer);
    expect(drawer.get('[role="alert"]').text()).toContain('题目已不存在，请刷新题库');
    expect(drawer.get<HTMLTextAreaElement>('textarea[aria-label="题目内容"]').element.value).toBe(
      '删除后仍保留的草稿',
    );
    expect(useIqTestStore().questions).toHaveLength(0);
    expect(saveRequest).not.toHaveBeenCalled();
  });
});
