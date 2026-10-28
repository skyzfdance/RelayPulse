import type { Connection } from '@/types/domain/sites';

import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';

import * as api from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteDetailsStore, useSiteStore } from '@/stores/site';

import AdvancedSettingsDrawer from '@/views/config/sites/components/AdvancedSettingsDrawer.vue';
import SiteAdvancedForm from '@/views/config/sites/components/SiteAdvancedForm.vue';

const dispose: (() => void)[] = [];
beforeEach(() => {
  setActivePinia(createPinia());
  seedSites();
  window.history.replaceState({}, '', '/');
  resetFeedbackForTests();
});
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
});
function mountForm(overrides: Partial<Connection> = {}) {
  const connection = reactive({ ...structuredClone(state.sites[0].connection), ...overrides });
  const wrapper = mount(SiteAdvancedForm, {
    attachTo: document.body,
    props: { modelValue: connection, baseUrl: 'https://example.com' },
    global: { plugins: [AntdvNext] },
  });
  dispose.push(() => wrapper.unmount());
  return { wrapper, connection };
}

describe('高级设置表单校验', () => {
  it.each([
    ['intervalSeconds', '探测频率', 86400],
    ['slowFirstTokenSeconds', '首 Token 慢阈值', 100],
    ['slowTotalSeconds', '总耗时慢阈值', 500],
    ['failureThreshold', '连续失败转红', 10],
    ['recoveryThreshold', '连续成功恢复', 10],
  ] as const)(
    '%s 在 change、失焦和保存校验时拒绝空白、非整数和越界值',
    async (name, label, max) => {
      const { wrapper, connection } = mountForm();
      const input = wrapper.get<HTMLInputElement>(`input#${name}`);
      const field = input.element.closest('.ant-form-item')!;
      for (const value of ['', '0', String(max + 1), '1.5', ' 2', '2 ', 'abc']) {
        await input.setValue('2');
        await input.setValue(value);
        await vi.waitFor(() =>
          expect(field.querySelector('.ant-form-item-explain-error')).not.toBeNull(),
        );
        await input.trigger('blur');
        await expect(wrapper.vm.validate()).rejects.toBeDefined();
        expect(field.textContent).toContain(label);
        expect(input.element.value).toBe(value);
      }
      for (const value of [1, max]) {
        await input.setValue(String(value));
        await input.trigger('blur');
        await expect(wrapper.vm.validate()).resolves.toBeDefined();
        expect(connection[name]).toBe(value);
        await vi.waitFor(() =>
          expect(field.querySelector('.ant-form-item-explain-error')).toBeNull(),
        );
      }
    },
  );

  it.each([
    ['modelsPath', '模型列表端点'],
    ['probePath', '探测端点'],
  ] as const)('%s 必填且拒绝首尾空格，修改后立即校验', async (name, label) => {
    const { wrapper } = mountForm();
    const input = wrapper.get(`input#${name}`);
    for (const value of ['', '   ', ' /v1/test', '/v1/test ']) {
      await input.setValue(value);
      await vi.waitFor(() =>
        expect(wrapper.text()).toContain(
          value.trim() ? `${label}首尾不能有空格` : `请输入${label}`,
        ),
      );
      await input.trigger('blur');
      await expect(wrapper.vm.validate()).rejects.toBeDefined();
    }
    await input.setValue('/v1/test');
    await expect(wrapper.vm.validate()).resolves.toBeDefined();
  });

  it('未修改的空字段在失焦时也显示必填提示', async () => {
    const { wrapper } = mountForm({ modelsPath: '' });
    expect(wrapper.text()).not.toContain('请输入模型列表端点');
    await wrapper.get('input#modelsPath').trigger('blur');
    await vi.waitFor(() => expect(wrapper.text()).toContain('请输入模型列表端点'));
  });

  it('请求头名称必填，值允许为空；非空名称和值拒绝首尾空格', async () => {
    const { wrapper, connection } = mountForm({ headers: [{ key: '', value: '' }] });
    const key = wrapper.get('[aria-label="请求头名称 1"]');
    const value = wrapper.get('[aria-label="请求头值 1"]');
    await key.trigger('blur');
    await value.trigger('blur');
    await vi.waitFor(() => expect(wrapper.text()).toContain('请输入请求头名称'));
    expect(wrapper.text()).not.toContain('请输入请求头值');
    await key.setValue(' X-Route');
    await vi.waitFor(() => expect(wrapper.text()).toContain('请求头名称首尾不能有空格'));
    await key.setValue('X-Route');
    for (const text of [' route', 'route ', '   ']) {
      await value.setValue(text);
      await vi.waitFor(() => expect(wrapper.text()).toContain('请求头值首尾不能有空格'));
      await expect(wrapper.vm.validate()).rejects.toBeDefined();
    }
    await value.setValue('');
    await expect(wrapper.vm.validate()).resolves.toBeDefined();
    expect(connection.headers).toEqual([{ key: 'X-Route', value: '' }]);
  });

  it('数字带空格时即使解析值没有改变，点击保存仍会校验并阻止关闭', async () => {
    await useSiteStore().loadSite({ autoLoadDetails: false });
    await useSiteDetailsStore().loadSiteDetails();
    const saveSite = vi.spyOn(api, 'saveSiteConnection');
    const wrapper = mount(AdvancedSettingsDrawer, {
      attachTo: document.body,
      global: { plugins: [AntdvNext] },
    });
    dispose.push(() => wrapper.unmount());
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    await drawer.get('input#intervalSeconds').setValue(' 300');
    const save = drawer
      .findAll('button')
      .find((button) => button.text().replace(/\s/gu, '') === '保存')!;
    expect(save.attributes('disabled')).toBeUndefined();
    await save.trigger('click');
    await vi.waitFor(() => expect(drawer.text()).toContain('探测频率首尾不能有空格'));
    expect(saveSite).not.toHaveBeenCalled();
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('保存按钮校验所有字段，失败时不调用保存；修正后允许空请求头值并以秒保存', async () => {
    state.sites[0].connection.modelsPath = '';
    state.sites[0].connection.headers = [{ key: '', value: '' }];
    state.sites[0].connection.failureThreshold = 11;
    await useSiteStore().loadSite({ autoLoadDetails: false });
    await useSiteDetailsStore().loadSiteDetails();
    const saveSite = vi.spyOn(api, 'saveSiteConnection');
    const wrapper = mount(AdvancedSettingsDrawer, {
      attachTo: document.body,
      global: { plugins: [AntdvNext] },
    });
    dispose.push(() => wrapper.unmount());
    await flushPromises();
    const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
    const save = drawer
      .findAll('button')
      .find((button) => button.text().replace(/\s/gu, '') === '保存')!;
    await save.trigger('click');
    await vi.waitFor(() => expect(drawer.findAll('.ant-form-item-explain-error')).toHaveLength(3));
    expect(saveSite).not.toHaveBeenCalled();
    expect(wrapper.emitted('close')).toBeUndefined();
    await drawer.get('input#modelsPath').setValue('/v1/models');
    await drawer.get('[aria-label="请求头名称 1"]').setValue('X-Route');
    await drawer.get('input#failureThreshold').setValue('10');
    await drawer.get('input#intervalSeconds').setValue('15');
    await save.trigger('click');
    await vi.waitFor(() => expect(saveSite).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(wrapper.emitted('close')).toHaveLength(1));
    expect(useSiteDetailsStore().selectedSiteDetail?.connection.intervalSeconds).toBe(15);
    expect((await api.fetchSite('site-1')).connection.headers[0].value).toBe('');
  });
});
