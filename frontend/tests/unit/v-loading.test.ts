import { mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { describe, expect, it } from 'vitest';
import { defineComponent, nextTick, ref } from 'vue';

import { vLoading } from '@/directives/loading';

describe('v-loading 指令', () => {
  it('按绑定值在节点内注入/移除遮罩并渲染 tip', async () => {
    const loading = ref(false);
    const Harness = defineComponent({
      setup: () => ({ loading, tip: '数据加载中...' }),
      template: `<div class="host" v-loading="{ loading, tip }"><p>内容</p></div>`,
    });

    const wrapper = mount(Harness, {
      global: { plugins: [AntdvNext], directives: { loading: vLoading } },
    });

    // 初始 false：节点内无遮罩
    expect(wrapper.find('.rp-loading-mask').exists()).toBe(false);

    // 置 true：遮罩注入，含 a-spin 与 tip，宿主补非 static 定位
    loading.value = true;
    await nextTick();
    expect(wrapper.find('.rp-loading-mask').exists()).toBe(true);
    expect(wrapper.find('.rp-loading-mask .ant-spin').exists()).toBe(true);
    expect(wrapper.find('.rp-loading-mask__tip').text()).toBe('数据加载中...');
    expect((wrapper.element as HTMLElement).style.position).toBe('relative');

    // 置回 false：遮罩移除
    loading.value = false;
    await nextTick();
    expect(wrapper.find('.rp-loading-mask').exists()).toBe(false);
  });
});
