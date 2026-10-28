import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import { createPanelMock } from '@/mocks/panel';

import PanelSurface from '@/views/panel/components/PanelSurface.vue';

describe('小面板展示数据边界', () => {
  it('直接消费等待与结论，按原顺序最多展示三个模型，保留分组且不按名称合并站点', async () => {
    const snapshot = createPanelMock('ready');
    snapshot.sites.reverse();
    snapshot.sites[1].models.reverse();
    snapshot.sites[1].models.push({
      ...snapshot.sites[1].models[0],
      id: 'panel-overflow-model',
      modelName: '排在第四位的模型',
    });
    snapshot.sites[0].siteName = snapshot.sites[1].siteName;
    const expectedSites = [
      snapshot.sites[0],
      { ...snapshot.sites[1], models: snapshot.sites[1].models.slice(0, 2) },
    ];
    const wrapper = mount(PanelSurface, { props: { snapshot } });

    expect(wrapper.find('.panel-summary').exists()).toBe(false);
    expect(wrapper.find('thead').exists()).toBe(false);
    expect(wrapper.findAll('tr[data-model-id]')).toHaveLength(3);
    expect(wrapper.get('.panel-list-caption').text()).toBe('显示 3/12 个模型');
    expect(wrapper.text()).not.toContain('排在第四位的模型');
    expect(snapshot.sites[1].models).toHaveLength(3);
    const groups = wrapper.findAll('tbody[data-site-id]');
    expect(groups.map((group) => group.attributes('data-site-id'))).toEqual(
      expectedSites.map((site) => site.siteId),
    );
    expect(
      groups.map((group) => group.findAll('.panel-model-name').map((model) => model.text())),
    ).toEqual(expectedSites.map((site) => site.models.map((model) => model.modelName)));
    expect(
      groups.map((group) => group.findAll('.panel-site-name').map((site) => site.text())),
    ).toEqual(expectedSites.map((site) => site.models.map(() => site.siteName)));
    expect(wrapper.find('[data-status="healthy"]').text()).toBe('正常');
    expect(wrapper.find('[data-status="slow"]').text()).toBe('偏慢');

    await wrapper.setProps({
      snapshot: {
        ...snapshot,
        sites: snapshot.sites.map((site) => ({
          ...site,
          models: site.models.map((model) => ({ ...model, status: 'healthy' })),
        })),
      },
    });
    expect(wrapper.find('table').exists()).toBe(true);
    expect(wrapper.find('.panel-summary').exists()).toBe(false);
    expect(wrapper.findAll('[data-status="healthy"]')).toHaveLength(3);

    // 等待快照即使携带旧结果，也不由展示层自动切换成结论态。
    await wrapper.setProps({ snapshot: { ...snapshot, state: 'waiting' } });
    expect(wrapper.get('h1').text()).toBe('等待探测');
    expect(wrapper.find('table').exists()).toBe(false);
    wrapper.unmount();
  });

  it('缺失耗时使用占位符，明确提供的零耗时仍按真实值显示', () => {
    const snapshot = createPanelMock('ready');
    snapshot.sites[0].models[0].totalMs = null;
    snapshot.sites[1].models[0].firstTokenMs = 0;
    const wrapper = mount(PanelSurface, { props: { snapshot } });

    expect(wrapper.get('[aria-label="未提供首 Token 耗时"]').text()).toBe('—');
    expect(wrapper.get('[aria-label="未提供总耗时"]').text()).toBe('—');
    const rows = wrapper.findAll('tr[data-model-id]');
    expect(rows[0].findAll('td')).toHaveLength(3);
    expect(rows[0].findAll('td')[2].text().replace(/\s/gu, '')).toBe('—/—');
    expect(rows[1].text()).toContain('0 ms');
    expect(wrapper.text()).toContain('6.79 s');
    wrapper.unmount();
  });

  it('已有结果但健康待确认时保留分组和真实耗时，不退回等待页或擅自判成不可用', () => {
    const snapshot = createPanelMock('ready');
    snapshot.sites = snapshot.sites.slice(0, 1);
    snapshot.siteCount = 1;
    snapshot.modelCount = 1;
    const wrapper = mount(PanelSurface, { props: { snapshot } });
    const group = wrapper.get(`tbody[data-site-id="${snapshot.sites[0].siteId}"]`);
    expect(group.get('.panel-site-name').text()).toBe(snapshot.sites[0].siteName);
    expect(group.get('[data-status="pending"]').text()).toBe('待确认');
    expect(group.get('.panel-duration-pair').text().replace(/\s/gu, '')).toBe('—/22.60s');
    expect(wrapper.find('.panel-summary').exists()).toBe(false);
    expect(wrapper.find('[data-status="unavailable"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('底部仅展示最近完成与调度文案，探测期间阻止重复刷新并保留其他动作接口', async () => {
    const snapshot = createPanelMock('ready', new Date(2026, 9, 26, 13, 57, 21));
    const wrapper = mount(PanelSurface, { props: { snapshot } });

    expect(wrapper.get('.panel-completion').text()).toContain('最近完成');
    expect(wrapper.get('.panel-completion time').text()).toBe('13:56:21');
    expect(wrapper.get('[role="status"]').text()).toBe('未启用周期探测');
    expect(wrapper.get('.panel-footer').text()).not.toContain('12/12');

    await wrapper.get('[aria-label="立即探测"]').trigger('click');
    await wrapper.get('[aria-label="打开设置"]').trigger('click');
    await wrapper.get('.panel-view-all').trigger('click');
    expect(wrapper.emitted('refresh')).toHaveLength(1);
    expect(wrapper.emitted('openSettings')).toHaveLength(1);
    expect(wrapper.emitted('viewAll')).toHaveLength(1);

    await wrapper.setProps({ refreshing: true });
    expect(wrapper.get('[role="status"]').text()).toBe('正在探测');
    expect(wrapper.get('[aria-label="立即探测"]').attributes('disabled')).toBeDefined();
    expect(snapshot.activeProbe).toBeNull();
    await wrapper.setProps({ refreshing: false });

    await wrapper.setProps({
      snapshot: { ...snapshot, activeProbe: { completedCount: 5, totalCount: 12 } },
    });
    expect(wrapper.get('[role="status"]').text()).toBe('正在探测');
    expect(wrapper.get('.panel-footer').text()).not.toContain('5/12');
    expect(wrapper.get('.panel-completion time').text()).toBe('13:56:21');
    expect(wrapper.get('[aria-label="立即探测"]').attributes('disabled')).toBeDefined();
    await wrapper.get('[aria-label="立即探测"]').trigger('click');
    expect(wrapper.emitted('refresh')).toHaveLength(1);

    await wrapper.setProps({
      snapshot: {
        ...snapshot,
        schedule: {
          state: 'scheduled',
          nextProbeAt: new Date(2026, 9, 26, 14, 0, 59).toISOString(),
        },
      },
    });
    expect(wrapper.get('[role="status"]').text()).toContain('下次');
    expect(wrapper.get('.panel-schedule time').text()).toBe('14:00');
    await wrapper.setProps({ snapshot: { ...snapshot, lastProbe: null } });
    expect(wrapper.get('[aria-label="尚未完成首次探测"]').text()).toBe('—');
    expect(wrapper.get('[role="status"]').text()).toBe('未启用周期探测');
    wrapper.unmount();
  });
});
