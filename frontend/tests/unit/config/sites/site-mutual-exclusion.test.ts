import { DOMWrapper, flushPromises, mount } from '@vue/test-utils';
import AntdvNext, {
  Button,
  Drawer,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Switch,
} from 'antdv-next';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import * as backend from '@/api/call-backend';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import {
  messagesValue,
  parameterKeySelector,
  parameterValueSelector,
  policyPrompt,
} from '../../../support/parameter-fields';

import AddModelModal from '@/views/config/sites/components/AddModelModal.vue';
import AdvancedSettingsDrawer from '@/views/config/sites/components/AdvancedSettingsDrawer.vue';
import ModelList from '@/views/config/sites/components/ModelList.vue';
import ModelProbeSettingsDrawer from '@/views/config/sites/components/ModelProbeSettingsDrawer.vue';
import ModelSyncModal from '@/views/config/sites/components/ModelSyncModal.vue';
import ProbePolicyFields from '@/views/config/sites/components/ProbePolicyFields.vue';
import SiteAdvancedForm from '@/views/config/sites/components/SiteAdvancedForm.vue';
import SiteBasicForm from '@/views/config/sites/components/SiteBasicForm.vue';
import SiteProbePolicyDrawer from '@/views/config/sites/components/SiteProbePolicyDrawer.vue';
import SitesView from '@/views/config/sites/SitesView.vue';

const dispose: (() => void)[] = [];
beforeEach(() => {
  setActivePinia(createPinia());
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  dispose.splice(0).forEach((fn) => fn());
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.history.replaceState({}, '', '/');
});
async function mountPage() {
  const wrapper = mount(SitesView, { attachTo: document.body, global: { plugins: [AntdvNext] } });
  dispose.push(() => wrapper.unmount());
  await flushPromises();
  await vi.waitFor(() => expect(wrapper.find('input#name').exists()).toBe(true), { timeout: 5000 });
  return wrapper;
}
type Page = Awaited<ReturnType<typeof mountPage>>;
function button(wrapper: Page, label: string) {
  return wrapper
    .findAll('button')
    .find((item) => (item.attributes('aria-label') ?? item.text().replace(/\s/gu, '')) === label)!;
}
function overlayButton(label: string) {
  return new DOMWrapper(
    [...document.querySelectorAll('.ant-drawer button, .ant-modal button')].find(
      (item) => item.textContent?.replace(/\s/gu, '') === label,
    )!,
  );
}
const writes = new Set([
  'sync_site_models',
  'apply_site_model_sync',
  'run_site_probe',
  'test_model_probe',
  'save_site',
  'update_site_connection',
  'update_site_probe_policy',
  'add_site_model',
  'delete_site_model',
  'set_site_model_enabled',
  'set_site_enabled',
  'delete_site',
]);
function delayWrites(fail = false) {
  window.history.replaceState({}, '', `/?siteDelay=1000${fail ? '&scenario=action-error' : ''}`);
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  const commands = vi.spyOn(backend, 'callBackend');
  return () => commands.mock.calls.filter(([command]) => writes.has(command));
}
async function finishRequest() {
  await vi.advanceTimersByTimeAsync(1000);
  await flushPromises();
}
function expectLocked(wrapper: Page) {
  for (const label of [
    '高级设置',
    '编辑默认值',
    '保存变更',
    '同步模型',
    '探测已启用模型',
    '添加模型',
    '设置模型 gpt-demo',
    '立即探测模型 gpt-demo',
    '删除模型 gpt-demo',
    '删除站点',
  ]) {
    expect(button(wrapper, label), `${label}: ${wrapper.text()}`).toBeDefined();
    expect(button(wrapper, label).attributes('disabled'), label).toBeDefined();
  }
  for (const input of wrapper.getComponent(SiteBasicForm).findAll('input'))
    expect(input.attributes('disabled')).toBeDefined();
  expect(wrapper.get('[aria-label="停用模型 gpt-demo"]').attributes('disabled')).toBeDefined();
}
async function forceBlockedEvents(wrapper: Page) {
  const list = wrapper.getComponent(ModelList);
  const before = JSON.stringify(list.emitted());
  // 直接触发组件回调，绕过 HTML disabled，验证事件本身被拦住。
  for (const item of list.findAllComponents(Button))
    item.vm.$emit('click', new MouseEvent('click'));
  for (const item of list.findAllComponents(Switch)) item.vm.$emit('change', false);
  for (const item of list.findAllComponents(Popconfirm)) item.vm.$emit('confirm');
  expect(JSON.stringify(list.emitted())).toBe(before);
  // 即使绕过 ModelList 的防护，父级操作入口也不得发送请求或打开编辑面板。
  list.vm.$emit('sync', 'site-1');
  list.vm.$emit('probe', 'site-1');
  list.vm.$emit('probe', 'site-1', 'model-0-0');
  list.vm.$emit('toggle', 'site-1', 'model-0-0', false);
  list.vm.$emit('remove', 'site-1', 'model-0-0');
  list.vm.$emit('add');
  list.vm.$emit('settings', 'model-0-0');
  for (const item of wrapper.findAllComponents(Button)) {
    if (['高级设置', '编辑默认值', '保存变更'].includes(item.text().replace(/\s/gu, '')))
      item.vm.$emit('click');
  }
  const basic = wrapper.getComponent(SiteBasicForm);
  const previous = JSON.stringify(basic.props('modelValue'));
  for (const input of basic.findAllComponents(Input)) input.vm.$emit('update:value', 'blocked');
  for (const select of basic.findAllComponents(Select))
    select.vm.$emit('update:value', 'anthropic');
  expect(JSON.stringify(basic.props('modelValue'))).toBe(previous);
  await flushPromises();
}

async function openCustomProbe(wrapper: Page) {
  await button(wrapper, '设置模型 gpt-demo').trigger('click');
  await flushPromises();
  const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
  await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
  await drawer.get('[aria-label="自定义探测端点"]').setValue('/draft-probe');
  await overlayButton('添加参数').trigger('click');
  await drawer.get(parameterKeySelector('')).setValue('prompt');
  await drawer.get(parameterValueSelector('prompt')).setValue('当前草稿');
  return drawer;
}

describe('站点操作互斥（真实 Mock 处理器延迟 1 秒）', () => {
  it.each(['sync', 'probe', 'probe-model'] as const)(
    '%s 期间拦截其余事件和配置变更，请求结束后恢复',
    async (type) => {
      const wrapper = await mountPage();
      const before = JSON.stringify(state.sites[0]);
      const requests = delayWrites();
      const label = {
        sync: '同步模型',
        probe: '探测已启用模型',
        'probe-model': '立即探测模型 gpt-demo',
      }[type];
      await button(wrapper, label).trigger('click');
      await flushPromises();
      expect(useSiteTasksStore().tasks['site-1']?.type).toBe(type);
      expect(JSON.stringify(state.sites[0])).toBe(before);
      expectLocked(wrapper);
      await forceBlockedEvents(wrapper);
      expect(requests()).toHaveLength(1);
      expect(wrapper.findComponent(AdvancedSettingsDrawer).exists()).toBe(false);
      expect(wrapper.findComponent(SiteProbePolicyDrawer).exists()).toBe(false);
      expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
      expect(wrapper.findComponent(AddModelModal).exists()).toBe(false);
      await finishRequest();
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(button(wrapper, '同步模型').attributes('disabled')).toBeUndefined();
      expect(button(wrapper, '探测已启用模型').attributes('disabled')).toBeUndefined();
      expect(wrapper.get('input#name').attributes('disabled')).toBeUndefined();
    },
  );

  it('试探当前草稿只发送一次请求，不保存配置；执行期间拦截所有关闭入口和冲突操作', async () => {
    const wrapper = await mountPage();
    const before = JSON.stringify(state.sites[0]);
    const drawer = await openCustomProbe(wrapper);
    const modelDrawer = wrapper.getComponent(ModelProbeSettingsDrawer);
    const componentButton = (label: string) =>
      modelDrawer
        .findAllComponents(Button)
        .find((item) => item.text().replace(/\s/gu, '') === label)!;
    const requests = delayWrites();
    componentButton('试探一次').vm.$emit('click');
    componentButton('试探一次').vm.$emit('click');
    componentButton('保存').vm.$emit('click');
    await flushPromises();
    expect(requests()).toHaveLength(1);
    expect(requests()[0].slice(0, 2)).toEqual([
      'test_model_probe',
      {
        siteId: 'site-1',
        modelId: 'model-0-0',
        request: {
          version: 2,
          endpoint: '/draft-probe',
          parameters: [
            { enabled: true, key: 'model', value: '', type: 'string', binding: 'model' },
            { enabled: true, key: 'prompt', type: 'string', value: '当前草稿' },
          ],
        },
      },
    ]);
    expect(useSiteTasksStore().tasks['site-1']?.type).toBe('test-model-probe');
    expectLocked(wrapper);
    await forceBlockedEvents(wrapper);
    const fields = modelDrawer.getComponent(ProbePolicyFields);
    const draft = JSON.stringify(fields.props('customRequest'));
    for (const input of fields.findAllComponents(Input)) input.vm.$emit('update:value', 'blocked');
    for (const select of fields.findAllComponents(Select))
      select.vm.$emit('update:value', 'number');
    for (const action of fields.findAllComponents(Button)) action.vm.$emit('click');
    modelDrawer.getComponent(Switch).vm.$emit('update:checked', false);
    expect(JSON.stringify(fields.props('customRequest'))).toBe(draft);
    expect(modelDrawer.getComponent(Switch).props('checked')).toBe(true);
    expect(componentButton('试探一次').props('loading')).toBe(true);
    expect(componentButton('取消').props('disabled')).toBe(true);
    expect(componentButton('保存').props('disabled')).toBe(true);
    const antDrawer = modelDrawer.getComponent(Drawer);
    expect(antDrawer.props('keyboard')).toBe(false);
    expect(antDrawer.props('closable')).toEqual({ disabled: true });
    expect(antDrawer.props('maskClosable')).toBe(false);
    componentButton('取消').vm.$emit('click');
    await drawer.get('.ant-drawer-close').trigger('click');
    await new DOMWrapper(document.querySelector('.ant-drawer-mask')!).trigger('click');
    await drawer.trigger('keydown', { key: 'Escape', keyCode: 27 });
    antDrawer.vm.$emit('close', new MouseEvent('click'));
    expect(modelDrawer.emitted('close')).toBeUndefined();
    // 即使绕过子组件直接触发 close，父组件也必须拒绝销毁抽屉。
    modelDrawer.vm.$emit('close');
    await wrapper.get('[data-sort-id="site-2"]').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(true);
    expect(useSiteStore().selectedSiteId).toBe('site-1');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(requests()).toHaveLength(1);
    expect(JSON.stringify(state.sites[0])).toBe(before);
    await finishRequest();
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
    expect(drawer.text()).toContain('模拟试探通过');
    expect(drawer.text()).toContain('HTTP 200');
    expect(drawer.text()).toContain('1.25 s');
    expect(drawer.text()).toContain('未向站点发送真实请求');
    expect(JSON.stringify(state.sites[0])).toBe(before);
    expect(useSiteDetailsStore().selectedSiteDetail?.models[0].customRequest).toBeNull();
    expect(componentButton('取消').props('disabled')).toBe(false);
    await drawer.get(parameterValueSelector('prompt')).setValue('修改后的草稿');
    expect(drawer.text()).not.toContain('试探通过');
    await overlayButton('取消').trigger('click');
    await flushPromises();
    expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
  });

  it.each(['request-error', 'http-error'] as const)(
    '试探 %s 后保留草稿并解除锁定，允许关闭',
    async (kind) => {
      const wrapper = await mountPage();
      const before = JSON.stringify(state.sites[0]);
      const drawer = await openCustomProbe(wrapper);
      const requests = delayWrites(kind === 'request-error');
      if (kind === 'http-error')
        window.history.replaceState({}, '', '/?siteDelay=1000&probeStatus=503');
      await overlayButton('试探一次').trigger('click');
      await flushPromises();
      expectLocked(wrapper);
      expect(overlayButton('取消').attributes('disabled')).toBeDefined();
      await finishRequest();
      expect(requests()).toHaveLength(1);
      expect(drawer.text()).toContain('试探失败');
      if (kind === 'http-error') expect(drawer.text()).toContain('HTTP 503');
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(drawer.get<HTMLInputElement>(parameterValueSelector('prompt')).element.value).toBe(
        '当前草稿',
      );
      expect(JSON.stringify(state.sites[0])).toBe(before);
      expect(overlayButton('试探一次').attributes('disabled')).toBeUndefined();
      expect(overlayButton('取消').attributes('disabled')).toBeUndefined();
      await overlayButton('取消').trigger('click');
      await flushPromises();
      expect(wrapper.findComponent(ModelProbeSettingsDrawer).exists()).toBe(false);
    },
  );

  it('无效端点或参数不能发出试探请求，常规探针不显示试探入口', async () => {
    const wrapper = await mountPage();
    await button(wrapper, '设置模型 gpt-demo').trigger('click');
    await flushPromises();
    const modelDrawer = wrapper.getComponent(ModelProbeSettingsDrawer);
    expect(modelDrawer.text()).not.toContain('试探一次');
    await new DOMWrapper(document.querySelector('[aria-label="使用自定义探针请求"]')!).trigger(
      'click',
    );
    const requests = delayWrites();
    const trial = modelDrawer.findAllComponents(Button).find((item) => item.text() === '试探一次')!;
    expect(trial.props('disabled')).toBe(true);
    trial.vm.$emit('click');
    await flushPromises();
    expect(requests()).toHaveLength(0);
    await new DOMWrapper(document.querySelector('[aria-label="自定义探测端点"]')!).setValue(
      '/custom',
    );
    await overlayButton('添加参数').trigger('click');
    trial.vm.$emit('click');
    await flushPromises();
    expect(requests()).toHaveLength(0);
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
  });

  it('同一轮事件中连续触发同步和探测，也只能发送一个请求', async () => {
    const wrapper = await mountPage();
    const requests = delayWrites();
    const list = wrapper.getComponent(ModelList);
    const action = (label: string) =>
      list.findAllComponents(Button).find((item) => item.text().replace(/\s/gu, '') === label)!;
    action('同步模型').vm.$emit('click');
    action('探测已启用模型').vm.$emit('click');
    action('同步模型').vm.$emit('click');
    await flushPromises();
    expect(requests().map(([command]) => command)).toEqual(['sync_site_models']);
    expect(list.emitted('probe')).toBeUndefined();
    expect(list.emitted('sync')).toHaveLength(1);
    await finishRequest();
  });

  it.each(['basic', 'connection', 'policy', 'model-policy', 'model-custom'] as const)(
    '%s 保存期间锁住所有冲突入口，重复点击不追加请求',
    async (kind) => {
      const wrapper = await mountPage();
      if (kind === 'basic') await wrapper.get('input#name').setValue('保存中的名称');
      else {
        const label = {
          connection: '高级设置',
          policy: '编辑默认值',
          'model-policy': '设置模型 gpt-demo',
          'model-custom': '设置模型 gpt-demo',
        }[kind];
        await button(wrapper, label).trigger('click');
        await flushPromises();
        if (kind === 'model-custom') {
          const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
          await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
          await drawer.get('[aria-label="自定义探测端点"]').setValue('/custom');
          await overlayButton('添加参数').trigger('click');
          await drawer.get(parameterKeySelector('')).setValue('prompt');
          await drawer.get(parameterValueSelector('prompt')).setValue('保存中的参数');
        }
        if (kind === 'connection')
          await new DOMWrapper(
            document.querySelector('.ant-drawer input#intervalSeconds')!,
          ).setValue('15');
        if (kind === 'policy')
          await new DOMWrapper(
            document.querySelector(`.ant-drawer ${parameterValueSelector('messages')}`)!,
          ).setValue(messagesValue('保存中的策略'));
      }
      const requests = delayWrites();
      await (kind === 'basic' ? button(wrapper, '保存变更') : overlayButton('保存')).trigger(
        'click',
      );
      await flushPromises();
      expect(requests()).toHaveLength(1);
      expectLocked(wrapper);
      await forceBlockedEvents(wrapper);
      if (kind !== 'basic') {
        wrapper
          .findAllComponents(Button)
          .find((item) => item.text().replace(/\s/gu, '') === '保存')!
          .vm.$emit('click');
        expect(overlayButton('保存').attributes('disabled')).toBeDefined();
        const fields =
          kind === 'connection'
            ? wrapper.getComponent(SiteAdvancedForm)
            : wrapper.getComponent(ProbePolicyFields);
        const savedInput = JSON.stringify(fields.props());
        for (const input of fields.findAllComponents(Input))
          input.vm.$emit('update:value', 'blocked');
        for (const input of fields.findAllComponents(InputNumber))
          input.vm.$emit('update:value', 99);
        for (const action of fields.findAllComponents(Button)) action.vm.$emit('click');
        expect(JSON.stringify(fields.props())).toBe(savedInput);
        if (kind === 'model-policy' || kind === 'model-custom') {
          const toggle = wrapper.getComponent(ModelProbeSettingsDrawer).getComponent(Switch);
          toggle.vm.$emit('update:checked', kind !== 'model-custom');
          await flushPromises();
          expect(toggle.props('checked')).toBe(kind === 'model-custom');
        }
      }
      await flushPromises();
      expect(requests()).toHaveLength(1);
      await finishRequest();
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(button(wrapper, '同步模型').attributes('disabled')).toBeUndefined();
      expect(button(wrapper, '探测已启用模型').attributes('disabled')).toBeUndefined();
      if (kind === 'basic') expect(state.sites[0].name).toBe('保存中的名称');
      if (kind === 'connection') expect(state.sites[0].connection.intervalSeconds).toBe(15);
      if (kind === 'policy') expect(policyPrompt(state.sites[0].policy)).toBe('保存中的策略');
      if (kind === 'model-custom')
        expect(state.sites[0].models[0].customRequest).toEqual({
          version: 2,
          endpoint: '/custom',
          parameters: [
            { enabled: true, key: 'model', value: '', type: 'string', binding: 'model' },
            { enabled: true, key: 'prompt', type: 'string', value: '保存中的参数' },
          ],
        });
    },
  );

  it.each(['高级设置', '编辑默认值', '设置模型 gpt-demo'])(
    '已打开 %s 时启动探测，抽屉禁止修改和保存',
    async (label) => {
      const wrapper = await mountPage();
      await button(wrapper, label).trigger('click');
      await flushPromises();
      const requests = delayWrites();
      wrapper.getComponent(ModelList).vm.$emit('probe', 'site-1');
      await flushPromises();
      expect(overlayButton('保存').attributes('disabled')).toBeDefined();
      const inputs = [...document.querySelectorAll<HTMLInputElement>('.ant-drawer input')];
      expect(inputs.length).toBeGreaterThan(0);
      expect(inputs.every((input) => input.disabled)).toBe(true);
      wrapper
        .findAllComponents(Button)
        .find((item) => item.text().replace(/\s/gu, '') === '保存')!
        .vm.$emit('click');
      const fields =
        label === '高级设置'
          ? wrapper.getComponent(SiteAdvancedForm)
          : wrapper.getComponent(ProbePolicyFields);
      const inputBefore = JSON.stringify(fields.props());
      for (const input of fields.findAllComponents(Input))
        input.vm.$emit('update:value', 'blocked');
      for (const input of fields.findAllComponents(InputNumber)) {
        input.vm.$emit('update:value', 99);
        input.vm.$emit('input', '99');
      }
      for (const select of fields.findAllComponents(Select))
        select.vm.$emit('update:value', 'number');
      expect(JSON.stringify(fields.props())).toBe(inputBefore);
      if (label === '设置模型 gpt-demo') {
        const toggle = wrapper.getComponent(ModelProbeSettingsDrawer).getComponent(Switch);
        toggle.vm.$emit('update:checked', true);
        await flushPromises();
        expect(toggle.props('checked')).toBe(false);
      }
      await flushPromises();
      expect(requests().map(([command]) => command)).toEqual(['run_site_probe']);
      await finishRequest();
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(overlayButton('保存').attributes('disabled')).toBeUndefined();
    },
  );

  it('同步确认仍走互斥入口，重复确认只发一次写入请求', async () => {
    const wrapper = await mountPage();
    await button(wrapper, '同步模型').trigger('click');
    await flushPromises();
    const confirmation = wrapper.getComponent(ModelSyncModal);
    const requests = delayWrites();
    const modal = confirmation.getComponent(Modal);
    modal.vm.$emit('ok');
    modal.vm.$emit('ok');
    await flushPromises();
    expect(requests().map(([command]) => command)).toEqual(['apply_site_model_sync']);
    expect(confirmation.emitted('confirm')).toHaveLength(1);
    expectLocked(wrapper);
    await forceBlockedEvents(wrapper);
    expect(requests()).toHaveLength(1);
    await finishRequest();
    expect(state.sites[0].models).toHaveLength(2);
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
  });

  it.each(['basic', 'connection', 'policy', 'model-policy', 'model-custom'] as const)(
    '%s 延迟保存失败后解除互斥，并保留编辑内容',
    async (kind) => {
      const wrapper = await mountPage();
      if (kind === 'basic') await wrapper.get('input#name').setValue('失败草稿');
      else {
        const label = {
          connection: '高级设置',
          policy: '编辑默认值',
          'model-policy': '设置模型 gpt-demo',
          'model-custom': '设置模型 gpt-demo',
        }[kind];
        await button(wrapper, label).trigger('click');
        await flushPromises();
        if (kind === 'model-custom') {
          const drawer = new DOMWrapper(document.querySelector('.ant-drawer')!);
          await drawer.get('[aria-label="使用自定义探针请求"]').trigger('click');
          await drawer.get('[aria-label="自定义探测端点"]').setValue('/custom');
          await overlayButton('添加参数').trigger('click');
          await drawer.get(parameterKeySelector('')).setValue('prompt');
          await drawer.get(parameterValueSelector('prompt')).setValue('保存中的参数');
        }
      }
      const requests = delayWrites(true);
      await (kind === 'basic' ? button(wrapper, '保存变更') : overlayButton('保存')).trigger(
        'click',
      );
      await flushPromises();
      expectLocked(wrapper);
      await finishRequest();
      expect(requests()).toHaveLength(1);
      expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
      expect(wrapper.get('input#name').attributes('disabled')).toBeUndefined();
      if (kind === 'basic') {
        expect(wrapper.get<HTMLInputElement>('input#name').element.value).toBe('失败草稿');
        expect(button(wrapper, '保存变更').attributes('disabled')).toBeUndefined();
      } else expect(overlayButton('保存').attributes('disabled')).toBeUndefined();
      if (kind === 'model-custom') {
        expect(
          document.querySelector<HTMLInputElement>(parameterValueSelector('prompt'))!.value,
        ).toBe('保存中的参数');
        expect(state.sites[0].models[0].customRequest).toBeNull();
      }
      expect(state.sites[0].name).toBe('OpenAI 主站');
    },
  );

  it.each(['同步模型', '探测已启用模型'])('%s 延迟失败后解除锁定，允许重新操作', async (label) => {
    const wrapper = await mountPage();
    const requests = delayWrites(true);
    if (label === '同步模型')
      window.history.replaceState({}, '', '/?siteDelay=1000&scenario=error');
    await button(wrapper, label).trigger('click');
    await flushPromises();
    expectLocked(wrapper);
    await finishRequest();
    expect(requests()).toHaveLength(1);
    expect(useSiteTasksStore().tasks['site-1']).toBeUndefined();
    expect(button(wrapper, '同步模型').attributes('disabled')).toBeUndefined();
    expect(button(wrapper, '探测已启用模型').attributes('disabled')).toBeUndefined();
    expect(wrapper.get('input#name').attributes('disabled')).toBeUndefined();
  });

  it('切换页面后任务仍互斥，返回时等待旧任务结束再更新详情', async () => {
    const first = await mountPage();
    const requests = delayWrites();
    await button(first, '探测已启用模型').trigger('click');
    await flushPromises();
    first.unmount();
    const next = await mountPage();
    expectLocked(next);
    await forceBlockedEvents(next);
    expect(requests()).toHaveLength(1);
    await finishRequest();
    expect(useSiteDetailsStore().selectedSiteDetail?.models[0].durationMs).toBe(1250);
    expect(button(next, '同步模型').attributes('disabled')).toBeUndefined();
  });
});
