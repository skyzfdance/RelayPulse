import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import AppIcon from '@/components/AppIcon.vue';

describe('AppIcon 全局图标组件', () => {
  it('按名称字符串渲染对应图标', () => {
    const wrapper = mount(AppIcon, { props: { name: 'BellOutlined' } });

    expect(wrapper.find('svg').exists()).toBe(true);
  });

  it('未知的名称渲染为空并输出告警', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const wrapper = mount(AppIcon, { props: { name: 'NotAnIcon' } });

    expect(wrapper.find('svg').exists()).toBe(false);
    expect(warn).toHaveBeenCalledOnce();
    expect(warn.mock.calls[0]?.[0]).toContain('NotAnIcon');
  });

  it('未传名称渲染为空', () => {
    const wrapper = mount(AppIcon);

    expect(wrapper.find('*').exists()).toBe(false);
  });
});
