import type { IqQuestion } from '@/types/domain/iq-test';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Button, Pagination } from 'antdv-next';
import { createPinia, disposePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createMemoryHistory, createRouter, RouterView } from 'vue-router';

import { resetEventBusForTests } from '@/api/events';
import * as bankApi from '@/api/iq-test/bank';
import * as taskApi from '@/api/iq-test/task';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites } from '@/mocks/handlers/config/sites';
import { iqState, seedIqState } from '@/mocks/handlers/iq-test';
import { useIqTestStore } from '@/stores/iq-test';

import IqTestView from '@/views/iq-test/task/IqTestView.vue';

const cleanup: (() => void)[] = [];
const content =
  '阅读以下代码并分析其边界行为：\nfunction next(value) {\n  return value + 1;\n}\n' +
  '请逐步解释空值、字符串和数值输入的差异，以及如何在不改变接口的情况下保证类型安全。'.repeat(6);
const answer = '先明确输入约束。\nfunction next(value: number): number {\n  return value + 1;\n}';
function questions(): IqQuestion[] {
  return Array.from({ length: 23 }, (_, index) => {
    const referenceAnswer = index === 0 ? answer : `参考答案 ${index + 1}`;
    return {
      id: `selection-question-${index + 1}`,
      content: index === 0 ? content : `分页测试题目 ${index + 1}`,
      referenceAnswer: index % 2 === 0 ? referenceAnswer : null,
      enabled: ![3, 11, 21].includes(index),
      createdAtMs: Date.UTC(2026, 9, 1),
      updatedAtMs: Date.UTC(2026, 9, 2, 10, index),
    };
  });
}

beforeEach(() => {
  window.history.replaceState({}, '', '/iq-test/task');
  resetEventBusForTests();
  resetFeedbackForTests();
  seedSites();
  seedIqState(true);
  iqState.questions = questions();
});
afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((fn) => fn());
  seedIqState();
  vi.restoreAllMocks();
  vi.useRealTimers();
  resetEventBusForTests();
  resetFeedbackForTests();
  document.body.innerHTML = '';
  window.history.replaceState({}, '', '/');
});

async function setup() {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/iq-test/task', component: IqTestView },
      { path: '/iq-test/bank', component: { template: '<div>题库</div>' } },
    ],
  });
  await router.push('/iq-test/task');
  await router.isReady();
  const wrapper = mount(RouterView, {
    attachTo: document.body,
    global: { plugins: [AntdvNext, pinia, router] },
  });
  cleanup.push(
    () => disposePinia(pinia),
    () => wrapper.unmount(),
  );
  await flushPromises();
  const store = useIqTestStore(pinia);
  await store.refresh();
  await flushPromises();
  const selector = () => wrapper.get('[aria-label="测试题目选择"]');
  const row = (number: number) => selector().get(`tr[data-row-key="selection-question-${number}"]`);
  const checkbox = (number: number) => row(number).get<HTMLInputElement>('input[type="checkbox"]');
  const count = () => selector().get('[aria-label="已选题目数量"]').text();
  const pager = () => wrapper.getComponent(Pagination);
  const search = async (keyword: string) => {
    await selector().get('input[aria-label="搜索测试题目"]').setValue(keyword);
    await flushPromises();
  };
  const page = async (number: number) => {
    await selector()
      .get(`[aria-label="题目选择分页"] li.ant-pagination-item-${number}`)
      .trigger('click');
    await flushPromises();
  };
  return { wrapper, store, selector, row, checkbox, count, pager, page, search };
}

describe('IQ 测试题目分页选择', () => {
  it('使用标准分页表格，保留题面原文及两层元信息，停用题目可见但不可选', async () => {
    const view = await setup();
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(10);
    expect(view.selector().get('input[type="search"]').attributes('aria-label')).toBe(
      '搜索测试题目',
    );
    expect(view.row(1).get('button[aria-label^="查看题目 "]').element.textContent).toContain(
      content,
    );
    expect(view.row(1).text()).toContain('有参考答案');
    expect(view.row(2).text()).toContain('无参考答案');
    expect(view.row(1).text()).toContain('更新于');
    expect(view.checkbox(4).element.disabled).toBe(true);
    expect(view.count()).toBe('已选 0 道 · 可选 20 道');
    expect(view.selector().find('.ant-table-wrapper .ant-pagination').exists()).toBe(false);
    expect(view.selector().get('[aria-label="题目选择分页"]').text()).toContain('共 23 道');
    await view.page(3);
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(3);
    expect(view.checkbox(22).element.disabled).toBe(true);
    expect(view.store.questions).toHaveLength(23);
  });

  it('翻页和改变每页数量保留全局已选集合，返回原页仍显示已勾选', async () => {
    const view = await setup();
    await view.checkbox(1).setValue(true);
    await view.page(2);
    expect(view.count()).toBe('已选 1 道 · 可选 20 道');
    await view.checkbox(11).setValue(true);
    await view.page(3);
    await view.checkbox(21).setValue(true);
    expect(view.count()).toBe('已选 3 道 · 可选 20 道');
    await view.page(1);
    expect(view.checkbox(1).element.checked).toBe(true);
    view.pager().vm.$emit('change', 1, 20);
    await flushPromises();
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(20);
    expect(view.checkbox(1).element.checked).toBe(true);
    expect(view.checkbox(11).element.checked).toBe(true);
    expect(view.count()).toBe('已选 3 道 · 可选 20 道');
    await view.page(2);
    expect(view.checkbox(21).element.checked).toBe(true);
    expect(view.store.questions).toHaveLength(23);
  });

  it('即时搜索全部原文，忽略首尾空格和英文大小写，清空恢复分页且保留每页数', async () => {
    iqState.questions[22]!.content += `\n${'完整题面内容。'.repeat(60)}\nAPI Plan 全文尾部关键字`;
    const originalQuestions = structuredClone(iqState.questions);
    const view = await setup();
    view.pager().vm.$emit('change', 1, 20);
    await flushPromises();
    await view.page(2);
    await view.search('  fUnCtIoN NeXt  ');
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('total')).toBe(1);
    expect(view.row(1).element).toBeDefined();
    await view.search('  api PLAN  ');
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('pageSize')).toBe(20);
    expect(view.pager().props('total')).toBe(1);
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(1);
    expect(view.row(23).text()).toContain('API Plan 全文尾部关键字');
    await view.search('全文尾部关键字');
    expect(view.pager().props('total')).toBe(1);
    expect(view.row(23).element).toBeDefined();

    await view.search('参考答案');
    expect(view.pager().props('total')).toBe(0);
    expect(view.selector().get('.ant-empty-normal').text()).toContain('暂无数据');
    expect(view.selector().text()).not.toContain('还没有测试题目');
    await view
      .selector()
      .findAll('button')
      .find((item) => item.text() === '清空搜索')!
      .trigger('click');
    await flushPromises();
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('pageSize')).toBe(20);
    expect(view.pager().props('total')).toBe(23);
    expect(
      view.selector().get<HTMLInputElement>('input[aria-label="搜索测试题目"]').element.value,
    ).toBe('');

    await view.page(2);
    await view.search('   ');
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('total')).toBe(23);
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(20);
    await view.search('分页测试');
    expect(view.pager().props('total')).toBe(22);
    await view.page(2);
    await view.selector().get('.ant-input-clear-icon').trigger('click');
    await flushPromises();
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('total')).toBe(23);
    expect(view.pager().props('pageSize')).toBe(20);
    expect(view.store.questions).toEqual(originalQuestions);
  });

  it('搜索隐藏的已选题目保持选择，匹配页全选和取消不改变隐藏或其他页选择', async () => {
    const view = await setup();
    await view.checkbox(1).setValue(true);
    await view.page(3);
    await view.checkbox(23).setValue(true);
    await view.search('分页测试');
    expect(view.pager().props('current')).toBe(1);
    expect(view.pager().props('total')).toBe(22);
    expect(view.selector().find('tr[data-row-key="selection-question-1"]').exists()).toBe(false);
    expect(view.count()).toBe('已选 2 道 · 可选 20 道');
    await view.checkbox(2).setValue(true);
    expect(view.count()).toBe('已选 3 道 · 可选 20 道');
    expect(view.checkbox(2).element.checked).toBe(true);
    await view.checkbox(2).setValue(false);
    expect(view.count()).toBe('已选 2 道 · 可选 20 道');
    expect(view.checkbox(2).element.checked).toBe(false);
    const all = () => view.selector().get<HTMLInputElement>('thead input[type="checkbox"]');
    await all().setValue(true);
    expect(view.count()).toBe('已选 11 道 · 可选 20 道');
    expect(view.checkbox(4).element.disabled).toBe(true);
    expect(view.checkbox(4).element.checked).toBe(false);
    expect(view.checkbox(11).element.checked).toBe(true);
    await all().setValue(false);
    expect(view.count()).toBe('已选 2 道 · 可选 20 道');
    await view.page(3);
    expect(view.checkbox(23).element.checked).toBe(true);
    await view.search('');
    expect(view.pager().props('current')).toBe(1);
    expect(view.checkbox(1).element.checked).toBe(true);
    await view.page(3);
    expect(view.checkbox(23).element.checked).toBe(true);
    expect(view.count()).toBe('已选 2 道 · 可选 20 道');
    expect(view.store.questions).toHaveLength(23);
  });

  it('表头全选仅操作当前页可用题，取消全选保留其他页的选择', async () => {
    const view = await setup();
    await view.checkbox(1).setValue(true);
    await view.page(2);
    const all = () => view.selector().get<HTMLInputElement>('thead input[type="checkbox"]');
    await all().setValue(true);
    expect(view.count()).toBe('已选 10 道 · 可选 20 道');
    expect(view.checkbox(11).element.checked).toBe(true);
    expect(view.checkbox(12).element.checked).toBe(false);
    expect(view.checkbox(12).element.disabled).toBe(true);
    await all().setValue(false);
    expect(view.count()).toBe('已选 1 道 · 可选 20 道');
    expect(view.checkbox(11).element.checked).toBe(false);
    await view.page(1);
    expect(view.checkbox(1).element.checked).toBe(true);
    expect(all().element.checked).toBe(false);
    expect(all().element.indeterminate).toBe(true);
  });

  it('同 ID 编辑保持选择，仅真实删除或停用清理对应已选题', async () => {
    const view = await setup();
    await view.checkbox(1).setValue(true);
    await view.page(2);
    await view.checkbox(11).setValue(true);
    await view.checkbox(13).setValue(true);
    await view.store.saveQuestion(
      {
        content: '编辑后仍保留选择的题目',
        referenceAnswer: '编辑后的答案',
        enabled: true,
      },
      'selection-question-11',
    );
    await flushPromises();
    expect(view.count()).toBe('已选 3 道 · 可选 20 道');
    expect(view.checkbox(11).element.checked).toBe(true);
    expect(view.row(11).text()).toContain('编辑后仍保留选择的题目');
    await view.store.deleteQuestion('selection-question-1');
    await flushPromises();
    expect(view.count()).toBe('已选 2 道 · 可选 19 道');
    await view.store.setQuestionEnabled('selection-question-13', false);
    await flushPromises();
    expect(view.count()).toBe('已选 1 道 · 可选 18 道');
    expect(view.checkbox(13).element.disabled).toBe(true);
    expect(view.checkbox(13).element.checked).toBe(false);
    expect(view.store.questions).toHaveLength(22);
    await view.page(1);
    expect(view.checkbox(11).element.checked).toBe(true);
  });

  it('末页题目删除后回退到有效页，仍保留其他页的选择', async () => {
    const view = await setup();
    await view.checkbox(1).setValue(true);
    await view.page(3);
    await view.checkbox(21).setValue(true);
    await view.store.deleteQuestion('selection-question-23');
    await view.store.deleteQuestion('selection-question-22');
    await view.store.deleteQuestion('selection-question-21');
    await flushPromises();
    expect(view.pager().props('current')).toBe(2);
    expect(view.selector().findAll('tbody tr[data-row-key]')).toHaveLength(10);
    expect(view.row(11).element).toBeDefined();
    expect(view.count()).toBe('已选 1 道 · 可选 18 道');
    await view.page(1);
    expect(view.checkbox(1).element.checked).toBe(true);
    expect(view.store.questions).toHaveLength(20);
  });

  it('题面及参考答案点击打开只读全文，保留代码换行且不改变勾选', async () => {
    const view = await setup();
    const save = vi.spyOn(bankApi, 'saveIqQuestion');
    await view.row(1).get('button[aria-label^="查看题目 "]').trigger('click');
    await flushPromises();
    const drawer = () => new DOMWrapper(document.querySelector('.ant-drawer')!);
    expect(drawer().get('[aria-label="题目全文"]').element.textContent).toContain(content);
    expect(drawer().get('[aria-label="题目参考答案"]').element.textContent).toContain(answer);
    expect(drawer().get('[aria-label="题目全文"] > div').classes()).toContain(
      'whitespace-pre-wrap',
    );
    expect(drawer().find('textarea').exists()).toBe(false);
    expect(drawer().find('.ant-drawer-footer .ant-btn-primary').exists()).toBe(false);
    expect(view.count()).toBe('已选 0 道 · 可选 20 道');
    await drawer().get('[aria-label="关闭题目抽屉"]').trigger('click');
    await flushPromises();
    await view.checkbox(1).setValue(true);
    await view.row(1).get('button[aria-label^="查看参考答案 "]').trigger('click');
    await flushPromises();
    expect(document.querySelectorAll('.ant-drawer')).toHaveLength(1);
    expect(drawer().get('[aria-label="题目参考答案"]').element.textContent).toContain(answer);
    expect(view.count()).toBe('已选 1 道 · 可选 20 道');
    expect(view.checkbox(1).element.checked).toBe(true);
    expect(save).not.toHaveBeenCalled();
  });

  it('从搜索结果开始时仍提交全部跨页和被搜索隐藏的已选 ID', async () => {
    const view = await setup();
    const start = vi.spyOn(taskApi, 'startIqRun');
    await view.checkbox(1).setValue(true);
    await view.page(3);
    await view.checkbox(21).setValue(true);
    await view.page(2);
    await view.checkbox(11).setValue(true);
    await view.search('分页测试题目 23');
    expect(view.pager().props('total')).toBe(1);
    expect(view.checkbox(23).element.checked).toBe(false);
    expect(view.count()).toBe('已选 3 道 · 可选 20 道');
    await view.store.addParticipant({
      modelKey: 'site-1/model-0-0',
      headers: [],
      preset: 'high',
      parameters: { reasoning_effort: 'high' },
    });
    await flushPromises();
    const button = view.wrapper
      .findAllComponents(Button)
      .find((item) => item.text() === '开始测试')!;
    expect(button.attributes('disabled')).toBeUndefined();
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    await button.trigger('click');
    await flushPromises();
    expect(start).toHaveBeenCalledTimes(1);
    expect(start.mock.calls[0]![0]).toEqual({ questionIds: expect.any(Array), retryMax: 5 });
    expect(start.mock.calls[0]![0].questionIds.slice().sort()).toEqual([
      'selection-question-1',
      'selection-question-11',
      'selection-question-21',
    ]);
    expect(view.store.activeRun?.questions.map((question) => question.id).sort()).toEqual([
      'selection-question-1',
      'selection-question-11',
      'selection-question-21',
    ]);
  });

  it('只读查看不因搜索隐藏关闭，随完整快照更新并在删除该题后关闭', async () => {
    const view = await setup();
    await view.row(1).get('button[aria-label^="查看题目 "]').trigger('click');
    await flushPromises();
    const drawer = () => new DOMWrapper(document.querySelector('.ant-drawer')!);
    await view.search('分页测试题目 23');
    expect(view.selector().find('tr[data-row-key="selection-question-1"]').exists()).toBe(false);
    expect(drawer().get('[aria-label="题目全文"]').element.textContent).toContain(content);
    await view.store.saveQuestion(
      {
        content: '更新后的第一行\n更新后的第二行',
        referenceAnswer: '新的参考答案\n第二步',
        enabled: true,
      },
      'selection-question-1',
    );
    await flushPromises();
    expect(drawer().get('[aria-label="题目全文"]').element.textContent).toContain(
      '更新后的第一行\n更新后的第二行',
    );
    expect(drawer().get('[aria-label="题目参考答案"]').element.textContent).toContain(
      '新的参考答案\n第二步',
    );
    await view.store.deleteQuestion('selection-question-1');
    await flushPromises();
    expect(document.querySelector('.ant-drawer')).toBeNull();
    expect(view.count()).toBe('已选 0 道 · 可选 19 道');
  });
});
