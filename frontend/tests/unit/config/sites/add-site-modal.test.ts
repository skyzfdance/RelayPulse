import type { SiteProtocolOption } from '@/types/domain/sites';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { App, Checkbox, ConfigProvider, Input, Modal, Select, theme } from 'antdv-next';
import zhCN from 'antdv-next/locale/zh_CN';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, ref } from 'vue';

import * as catalog from '@/api/catalog';
import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites } from '@/mocks/handlers/config/sites';
import { useSiteStore } from '@/stores/site';

import { deferred } from '../../../support/deferred';

import AddSiteModal from '@/views/config/sites/components/AddSiteModal.vue';

const dispose: (() => void)[] = [];
beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(async () => {
  dispose.splice(0).forEach((fn) => fn());
  await flushPromises();
  vi.restoreAllMocks();
});

async function mountModal() {
  const wrapper = mount(AddSiteModal, {
    attachTo: document.body,
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  await flushPromises();
  const modal = new DOMWrapper(document.querySelector('.ant-modal')!);
  return { wrapper, modal };
}

async function fillFields(modal: Awaited<ReturnType<typeof mountModal>>['modal']) {
  await modal.get('input#name').setValue('新站点');
  await modal.get('input#baseUrl').setValue('https://example.com');
  await modal.get('input[type="password"]').setValue('test-only-secret');
}

async function submit(modal: Awaited<ReturnType<typeof mountModal>>['modal']) {
  await modal.get('.ant-modal-footer .ant-btn-primary').trigger('click');
  await flushPromises();
}

describe('添加站点的表单校验和协议目录', () => {
  it('打开、协议返回、表单校验和关闭不产生 Vue 运行时警告', async () => {
    const warnings = vi.spyOn(console, 'warn');
    const host = document.createElement('div');
    document.body.append(host);
    const open = ref(false);
    const app = createApp({
      render: () =>
        h(
          ConfigProvider,
          { locale: zhCN, theme: { cssVar: true, algorithm: theme.darkAlgorithm } },
          {
            default: () =>
              h(App, null, {
                default: () =>
                  open.value
                    ? h(AddSiteModal, {
                        onClose: () => {
                          open.value = false;
                        },
                      })
                    : null,
              }),
          },
        ),
    });
    app.use(createPinia());
    app.use(AntdvNext);
    app.mount(host);
    dispose.push(() => {
      app.unmount();
      host.remove();
    });
    open.value = true;
    await flushPromises();
    const modal = new DOMWrapper(document.querySelector('.ant-modal')!);
    await vi.waitFor(() => expect(document.activeElement).toBe(modal.get('input#name').element));
    await modal.get('input#name').trigger('blur');
    await submit(modal);
    await fillFields(modal);
    await flushPromises();
    await modal.get('.ant-modal-close').trigger('click');
    await flushPromises();
    expect(
      warnings.mock.calls.filter(([message]) => String(message).includes('[Vue warn]')),
    ).toEqual([]);
  });

  it('直接触发重复提交、取消和字段事件时，进行中的创建请求也不会重复或改变输入', async () => {
    const pending = deferred<string>();
    const create = vi.spyOn(api, 'createSite').mockReturnValueOnce(pending.promise);
    const { wrapper, modal } = await mountModal();
    await vi.waitFor(() => expect(wrapper.getComponent(Select).props('options')).toHaveLength(4));
    await fillFields(modal);
    const component = wrapper.getComponent(Modal);
    component.vm.$emit('ok');
    component.vm.$emit('ok');
    await flushPromises();
    expect(create).toHaveBeenCalledOnce();
    const submitted = { ...create.mock.calls[0][0] };
    component.vm.$emit('cancel');
    component.vm.$emit('ok');
    for (const input of wrapper.findAllComponents(Input)) input.vm.$emit('update:value', 'blocked');
    wrapper.getComponent(Select).vm.$emit('update:value', 'anthropic');
    wrapper.getComponent(Checkbox).vm.$emit('update:checked', false);
    await flushPromises();
    expect(create).toHaveBeenCalledOnce();
    expect(create.mock.calls[0][0]).toEqual(submitted);
    expect(wrapper.emitted('close')).toBeUndefined();
    expect(modal.get<HTMLInputElement>('input#name').element.value).toBe('新站点');
    expect(modal.get('input#name').attributes('disabled')).toBeDefined();
    expect(wrapper.getComponent(Select).props('value')).toBe(submitted.protocol);
    expect(wrapper.getComponent(Checkbox).props('checked')).toBe(true);
    pending.reject(new Error('连接失败'));
    await flushPromises();
    expect(modal.get('input#name').attributes('disabled')).toBeUndefined();
    component.vm.$emit('cancel');
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('创建请求结束时弹窗已经卸载，不改变其他页面的选中项', async () => {
    const store = useSiteStore();
    await store.loadSite();
    const pending = deferred<string>();
    vi.spyOn(api, 'createSite').mockReturnValueOnce(pending.promise);
    const { wrapper, modal } = await mountModal();
    await fillFields(modal);
    await submit(modal);
    wrapper.unmount();
    dispose.pop();
    store.selectedSiteId = 'site-2';
    pending.resolve('site-1');
    await flushPromises();
    expect(store.selectedSiteId).toBe('site-2');
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('显示四项必填标记，失焦和提交展示中文提示；校验失败不发起添加或列表刷新', async () => {
    vi.spyOn(catalog, 'fetchSiteProtocols').mockResolvedValue([]);
    const create = vi.spyOn(api, 'createSite');
    const load = vi.spyOn(useSiteStore(), 'loadSite');
    const { modal, wrapper } = await mountModal();
    expect(modal.findAll('label.ant-form-item-required')).toHaveLength(4);

    await modal.get('input#name').trigger('blur');
    await flushPromises();
    expect(modal.text()).toContain('请输入站点名称');
    await submit(modal);
    for (const message of [
      '请输入站点名称',
      '请选择接口协议',
      '请输入 Base URL',
      '请输入访问凭据',
    ]) {
      expect(modal.text()).toContain(message);
    }
    expect(create).not.toHaveBeenCalled();
    expect(load).not.toHaveBeenCalled();
    expect(wrapper.emitted('close')).toBeUndefined();
    expect(modal.get('.ant-modal-footer .ant-btn-primary').classes()).not.toContain(
      'ant-btn-loading',
    );
  });

  it('Base URL 非空但格式错误时展示格式提示，不提交', async () => {
    const create = vi.spyOn(api, 'createSite');
    const { modal } = await mountModal();
    await fillFields(modal);
    await modal.get('input#baseUrl').setValue('invalid');
    await modal.get('input#baseUrl').trigger('blur');
    await flushPromises();
    expect(modal.text()).toContain('请输入有效的 Base URL');
    await submit(modal);
    expect(create).not.toHaveBeenCalled();
  });

  it('添加和刷新期间确定按钮持续 loading 并阻止重复点击，失败后可重试', async () => {
    let rejectCreate!: (error: Error) => void;
    const create = vi.spyOn(api, 'createSite').mockReturnValueOnce(
      new Promise<string>((_resolve, reject) => {
        rejectCreate = reject;
      }),
    );
    const store = useSiteStore();
    const loadSite = store.loadSite;
    let finishLoad!: () => void;
    const load = vi.spyOn(store, 'loadSite').mockImplementationOnce(async () => {
      await new Promise<void>((resolve) => {
        finishLoad = resolve;
      });
      await loadSite({ autoLoadDetails: false });
    });
    const { wrapper, modal } = await mountModal();
    const button = () => modal.get('.ant-modal-footer .ant-btn-primary');
    await fillFields(modal);
    await submit(modal);
    expect(button().classes()).toContain('ant-btn-loading');
    await submit(modal);
    expect(create).toHaveBeenCalledOnce();

    rejectCreate(new Error('添加失败'));
    await flushPromises();
    expect(button().classes()).not.toContain('ant-btn-loading');
    expect(load).not.toHaveBeenCalled();
    expect(wrapper.emitted('close')).toBeUndefined();

    await submit(modal);
    await vi.waitFor(() => expect(load).toHaveBeenCalledOnce());
    expect(button().classes()).toContain('ant-btn-loading');
    await submit(modal);
    expect(create).toHaveBeenCalledTimes(2);

    finishLoad();
    await vi.waitFor(() => expect(wrapper.emitted('close')).toHaveLength(1));
    expect(button().classes()).not.toContain('ant-btn-loading');
  });

  it('选项来自接口，失败保留输入和选中项，成功后按返回的新 ID 选中站点', async () => {
    const options: SiteProtocolOption[] = [
      { value: 'anthropic', label: '接口返回的协议名称' },
      { value: 'custom-http', label: '接口返回的自定义协议' },
    ];
    const fetch = vi.spyOn(catalog, 'fetchSiteProtocols').mockResolvedValue(options);
    const create = vi.spyOn(api, 'createSite');
    const store = useSiteStore();
    await store.loadSite();
    const selectedSiteId = store.sites[1].id;
    store.selectedSiteId = selectedSiteId;
    const load = vi.spyOn(store, 'loadSite');
    const { wrapper, modal } = await mountModal();
    expect(fetch).toHaveBeenCalledOnce();
    expect(wrapper.getComponent(Select).props('options')).toEqual(options);
    expect(modal.text()).toContain(options[0].label);
    await fillFields(modal);
    window.history.replaceState({}, '', '/?scenario=action-error');
    await submit(modal);
    expect(create).toHaveBeenCalledWith(expect.objectContaining({ protocol: 'anthropic' }));
    await expect(create.mock.results[0]?.value).rejects.toThrow();
    await flushPromises();
    expect(load).not.toHaveBeenCalled();
    expect(store.selectedSiteId).toBe(selectedSiteId);
    expect(wrapper.emitted('close')).toBeUndefined();
    expect(modal.get<HTMLInputElement>('input#name').element.value).toBe('新站点');

    window.history.replaceState({}, '', '/');
    await submit(modal);
    const siteId = await create.mock.results[1]?.value;
    expect(siteId).toEqual(expect.any(String));
    await flushPromises();
    expect(load).toHaveBeenCalledOnce();
    expect(store.sites.find((site) => site.id === siteId)?.name).toBe('新站点');
    expect(store.selectedSiteId).toBe(siteId);
  });

  it('协议读取失败时不回退固定选项；重新打开可以再次取得协议', async () => {
    const fetch = vi
      .spyOn(catalog, 'fetchSiteProtocols')
      .mockRejectedValueOnce(new Error('协议读取失败'))
      .mockResolvedValueOnce([{ value: 'openai-responses', label: '恢复后的协议' }]);
    const create = vi.spyOn(api, 'createSite');
    const first = await mountModal();
    expect(first.wrapper.getComponent(Select).props('options')).toEqual([]);
    await fillFields(first.modal);
    await submit(first.modal);
    expect(first.modal.text()).toContain('请选择接口协议');
    expect(create).not.toHaveBeenCalled();
    first.wrapper.unmount();
    dispose.pop();

    const next = await mountModal();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(next.modal.text()).toContain('恢复后的协议');
  });
});
