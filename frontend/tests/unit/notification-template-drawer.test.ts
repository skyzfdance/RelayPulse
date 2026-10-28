import type { NotificationTemplateDetail } from '@/types/domain/notifications';

import { flushPromises, mount, shallowMount } from '@vue/test-utils';
import AntdvNext, { Input, Segmented } from 'antdv-next';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';

import { fetchNotificationMarkers, fetchNotificationVariables } from '@/api/catalog';
import {
  fetchNotificationSettings,
  fetchNotificationTemplateDetail,
  sendTestNotification,
  updateNotificationTemplate,
} from '@/api/config/notifications';

import NotificationTemplateDrawer from '@/views/config/notifications/components/NotificationTemplateDrawer.vue';
import NotificationsView from '@/views/config/notifications/NotificationsView.vue';

vi.mock('@/api/catalog', () => ({
  fetchNotificationMarkers: vi.fn(),
  fetchNotificationVariables: vi.fn(),
}));
vi.mock('@/api/config/notifications', () => ({
  fetchNotificationSettings: vi.fn(),
  fetchNotificationTemplateDetail: vi.fn(),
  updateNotificationTemplate: vi.fn(),
  sendTestNotification: vi.fn(),
}));

const detail: NotificationTemplateDetail = {
  id: 'probe.model_failed',
  title: '模型探测故障',
  subTitle: '探测故障通知',
  status: true,
  channel: '系统通知',
  templateType: 'custom',
  markerId: 'none',
  titleTemplate: '自定义标题',
  bodyTemplate: '自定义正文',
  defaultMarkerId: 'warning',
  defaultTitleTemplate: '默认标题',
  defaultBodyTemplate: '默认正文',
};

const DrawerStub = defineComponent({
  props: { closable: { type: Object, default: undefined }, keyboard: Boolean, open: Boolean },
  emits: ['close'],
  template: '<div><slot name="title" /><slot name="extra" /><slot /></div>',
});

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(fetchNotificationTemplateDetail).mockImplementation(async () => ({ ...detail }));
  vi.mocked(fetchNotificationMarkers).mockResolvedValue([
    { markerId: 'none', symbol: '', name: '无' },
    { markerId: 'warning', symbol: '⚠️', name: '警告' },
  ]);
  vi.mocked(fetchNotificationVariables).mockResolvedValue([]);
  vi.mocked(fetchNotificationSettings).mockResolvedValue({
    notificationsEnabled: true,
    authorization: 'granted',
    rules: [{ ...detail }],
  });
});

async function mountDrawer() {
  const wrapper = mount(NotificationTemplateDrawer, {
    props: { open: true, ruleId: detail.id, testAvailable: true },
    global: { plugins: [AntdvNext], stubs: { ADrawer: DrawerStub, AppIcon: true } },
  });
  await flushPromises();
  return wrapper;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function button(wrapper: Awaited<ReturnType<typeof mountDrawer>>, text: string) {
  return wrapper.findAll('button').find((item) => item.text().replace(/\s/gu, '') === text)!;
}

describe('通知模板抽屉', () => {
  it.each([
    ['title', '\u007f'],
    ['body', '\u0085'],
    ['title', '\u2028'],
    ['title', '\u2029'],
  ])('字段 %s 含非法字符 %j 时提示并阻止保存和测试，修正后恢复', async (field, character) => {
    const wrapper = await mountDrawer();
    const input =
      field === 'title' ? wrapper.findComponent(Input).find('input') : wrapper.find('textarea');
    await input.setValue(`通知${character}内容`);
    expect(wrapper.text()).toContain('不能包含控制字符或换行');
    expect(button(wrapper, '保存').attributes('disabled')).toBeDefined();
    expect(button(wrapper, '测试当前模板').attributes('disabled')).toBeDefined();
    await button(wrapper, '保存').trigger('click');
    await button(wrapper, '测试当前模板').trigger('click');
    expect(updateNotificationTemplate).not.toHaveBeenCalled();
    expect(sendTestNotification).not.toHaveBeenCalled();
    await input.setValue('正常内容');
    expect(button(wrapper, '保存').attributes('disabled')).toBeUndefined();
    expect(button(wrapper, '测试当前模板').attributes('disabled')).toBeUndefined();
    wrapper.unmount();
  });

  it('展开正文超长时提示并禁止测试，但仍可保存原模板', async () => {
    vi.mocked(fetchNotificationVariables).mockResolvedValue([
      { key: 'app', label: '应用名', description: '', example: 'RelayPulse', group: '通用变量' },
    ]);
    vi.mocked(updateNotificationTemplate).mockResolvedValue({ ...detail });
    const wrapper = await mountDrawer();
    const body = '{{app}}'.repeat(50) + '!';
    await wrapper.find('textarea').setValue(body);
    expect(wrapper.text()).toContain('测试正文：不能超过 500 个字符');
    expect(button(wrapper, '测试当前模板').attributes('disabled')).toBeDefined();
    expect(button(wrapper, '保存').attributes('disabled')).toBeUndefined();
    await button(wrapper, '测试当前模板').trigger('click');
    expect(sendTestNotification).not.toHaveBeenCalled();
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(updateNotificationTemplate).toHaveBeenCalledWith(detail.id, {
      templateType: 'custom',
      markerId: 'none',
      titleTemplate: detail.titleTemplate,
      bodyTemplate: body,
    });
    expect(wrapper.emitted('saved')).toEqual([[]]);
    wrapper.unmount();
  });

  it('测试正文缩短至 500 个书写符号后恢复发送，并只发送一次标记', async () => {
    vi.mocked(fetchNotificationVariables).mockResolvedValue([
      { key: 'app', label: '应用名', description: '', example: 'RelayPulse', group: '通用变量' },
    ]);
    vi.mocked(fetchNotificationTemplateDetail).mockResolvedValue({
      ...detail,
      markerId: 'warning',
    });
    vi.mocked(sendTestNotification).mockResolvedValue();
    const wrapper = await mountDrawer();
    await wrapper.find('textarea').setValue('{{app}}'.repeat(50) + '!');
    await wrapper.find('textarea').setValue('{{app}}'.repeat(50));
    expect(wrapper.text()).not.toContain('测试正文：');
    expect(button(wrapper, '测试当前模板').attributes('disabled')).toBeUndefined();
    await button(wrapper, '测试当前模板').trigger('click');
    expect(sendTestNotification).toHaveBeenCalledWith({
      title: '⚠️ 自定义标题',
      body: 'RelayPulse'.repeat(50),
      markerId: 'warning',
    });
    wrapper.unmount();
  });

  it('组合 emoji 由 grapheme 校验，不被输入框 UTF-16 上限截断', async () => {
    const wrapper = await mountDrawer();
    const title = '👩🏽‍💻'.repeat(120);
    const body = '👩🏽‍💻'.repeat(500);
    expect(wrapper.findComponent(Input).find('input').attributes('maxlength')).toBeUndefined();
    expect(wrapper.find('textarea').attributes('maxlength')).toBeUndefined();
    await wrapper.findComponent(Input).find('input').setValue(title);
    await wrapper.find('textarea').setValue(body);
    expect(wrapper.findComponent(Input).find('input').element.value).toBe(title);
    expect(wrapper.find('textarea').element.value).toBe(body);
    expect(button(wrapper, '保存').attributes('disabled')).toBeUndefined();
    expect(button(wrapper, '测试当前模板').attributes('disabled')).toBeUndefined();
    await wrapper
      .findComponent(Input)
      .find('input')
      .setValue(title + 'x');
    expect(button(wrapper, '保存').attributes('disabled')).toBeDefined();
    await wrapper.findComponent(Input).find('input').setValue(title);
    await wrapper.find('textarea').setValue(body + 'x');
    expect(button(wrapper, '保存').attributes('disabled')).toBeDefined();
    wrapper.unmount();
  });
  it('打开默认模式时直接使用接口返回的默认字段', async () => {
    vi.mocked(fetchNotificationTemplateDetail).mockResolvedValueOnce({
      ...detail,
      templateType: 'default',
    });
    const wrapper = await mountDrawer();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('默认标题');
    expect(wrapper.find('textarea').element.value).toBe('默认正文');
    wrapper.unmount();
  });

  it('切回默认直接覆盖表单，切到自定义继续编辑当前默认值', async () => {
    const wrapper = await mountDrawer();
    await wrapper.findComponent(Input).find('input').setValue('未保存的修改');
    wrapper.findComponent(Segmented).vm.$emit('change', 'default');
    await flushPromises();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('默认标题');
    expect(wrapper.find('textarea').element.value).toBe('默认正文');
    expect(wrapper.text()).toContain('⚠️ 默认标题');
    expect(wrapper.findComponent(Input).props('disabled')).toBe(true);

    wrapper.findComponent(Segmented).vm.$emit('change', 'custom');
    await flushPromises();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('默认标题');
    expect(wrapper.findComponent(Input).props('disabled')).toBe(false);
    wrapper.unmount();
  });

  it('保存期间禁止编辑和关闭，成功后关闭并发送刷新事件', async () => {
    const pending = deferred<NotificationTemplateDetail>();
    vi.mocked(updateNotificationTemplate).mockReturnValue(pending.promise);
    const wrapper = await mountDrawer();
    await wrapper.findComponent(Input).find('input').setValue('新的标题');
    await button(wrapper, '保存').trigger('click');
    expect(updateNotificationTemplate).toHaveBeenCalledWith(detail.id, {
      templateType: 'custom',
      markerId: 'none',
      titleTemplate: '新的标题',
      bodyTemplate: '自定义正文',
    });
    expect(wrapper.findComponent(Input).props('disabled')).toBe(true);
    expect(button(wrapper, '取消').attributes('disabled')).toBeDefined();
    expect(wrapper.findComponent(DrawerStub).props('closable')).toEqual({ disabled: true });
    expect(wrapper.findComponent(DrawerStub).props('keyboard')).toBe(false);
    wrapper.findComponent(DrawerStub).vm.$emit('close');
    await flushPromises();
    expect(wrapper.emitted('close')).toBeUndefined();

    pending.resolve({ ...detail });
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(wrapper.emitted('saved')).toEqual([[]]);
    wrapper.unmount();
  });

  it('保存失败保留表单并恢复编辑和关闭', async () => {
    vi.mocked(updateNotificationTemplate).mockRejectedValue(new Error('保存失败'));
    const wrapper = await mountDrawer();
    await wrapper.findComponent(Input).find('input').setValue('保留这个修改');
    await button(wrapper, '保存').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('保留这个修改');
    expect(wrapper.findComponent(Input).props('disabled')).toBe(false);
    expect(wrapper.emitted('saved')).toBeUndefined();
    await button(wrapper, '取消').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });

  it('关闭后重新加载，不沿用未保存的内容；旧加载不会覆盖新表单', async () => {
    const old = deferred<NotificationTemplateDetail>();
    vi.mocked(fetchNotificationTemplateDetail).mockReturnValueOnce(old.promise);
    const wrapper = await mountDrawer();
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true, ruleId: 'probe.model_recovered' });
    await flushPromises();
    await wrapper.findComponent(Input).find('input').setValue('新的编辑');
    old.resolve({ ...detail, titleTemplate: '旧请求' });
    await flushPromises();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('新的编辑');
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(wrapper.findComponent(Input).find('input').element.value).toBe('自定义标题');
    expect(fetchNotificationTemplateDetail).toHaveBeenCalledTimes(3);
    wrapper.unmount();
  });

  it('测试通知发送后立即允许关闭，不等待请求完成', async () => {
    const pending = deferred<void>();
    vi.mocked(sendTestNotification).mockReturnValue(pending.promise);
    const wrapper = await mountDrawer();
    await button(wrapper, '测试当前模板').trigger('click');
    expect(sendTestNotification).toHaveBeenCalledWith({
      title: '自定义标题',
      body: '自定义正文',
      markerId: 'none',
    });
    expect(wrapper.findComponent(Input).props('disabled')).toBe(false);
    await button(wrapper, '取消').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
    pending.resolve();
    await flushPromises();
  });

  it('父页面收到保存事件后重新读取通知设置', async () => {
    const wrapper = shallowMount(NotificationsView);
    await flushPromises();
    expect(fetchNotificationSettings).toHaveBeenCalledTimes(1);
    wrapper.findComponent(NotificationTemplateDrawer).vm.$emit('saved');
    await flushPromises();
    expect(fetchNotificationSettings).toHaveBeenCalledTimes(2);
    wrapper.unmount();
  });
});
