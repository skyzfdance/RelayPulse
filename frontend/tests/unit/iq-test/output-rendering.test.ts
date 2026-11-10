import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { describe, expect, it, vi } from 'vitest';

import { outputBlocks, previewDocument } from '@/views/iq-test/history/iq-output-renderer';

import IqOutputCode from '@/views/iq-test/history/IqOutputCode.vue';
import IqOutputPreview from '@/views/iq-test/history/IqOutputPreview.vue';

describe('IQ 保存内容的阅读与预览', () => {
  it('渲染表格和两种公式分隔符，HTML 和危险链接不会进入宿主页面', () => {
    const blocks = outputBlocks(
      '# 答案\n\n| 数量 |\n|---|\n| 21 |\n\n\\(9+12=21\\)\n\n\\[\\boxed{21}\\]\n\n<script>window.evil=1</script>\n\n[x](javascript:alert(1))',
    );
    const html = blocks
      .filter((block) => block.kind === 'markdown')
      .map((block) => block.html)
      .join('');
    expect(html).toContain('<table>');
    expect(html).toContain('katex');
    expect(html).not.toContain('<script>');
    expect(html).not.toContain('href="javascript:');
  });
  it('代码内容保真，HTML/SVG 代码单独交给隔离预览', () => {
    const source = '<svg><text>\\(literal\\)</text></svg>\n';
    expect(outputBlocks('```svg\n' + source + '```')).toEqual([
      { kind: 'code', language: 'svg', content: source },
    ]);
    const document = previewDocument(source, 'preview-test');
    expect(document).toContain('Content-Security-Policy');
    expect(document).toContain("connect-src 'none'");
    expect(document).toContain(source);
  });

  it('按可用宽高完整缩放作品，忽略其他窗口和旧预览的消息', async () => {
    const width = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(600);
    const height = vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(240);
    const wrapper = mount(IqOutputPreview, {
      props: { content: '<html><body>完整作品</body></html>' },
      attachTo: document.body,
    });
    try {
      const frame = wrapper.get<HTMLIFrameElement>('iframe');
      const canvas = frame.element.parentElement!;
      const channel = JSON.parse(frame.element.srcdoc.match(/const channel = ("[^"]+")/)![1]!);
      const report = async (
        height: number,
        source = frame.element.contentWindow,
        identity = channel,
      ) => {
        window.dispatchEvent(
          new MessageEvent('message', {
            source,
            data: { type: 'iq-preview-size', channel: identity, height },
          }),
        );
        await flushPromises();
      };
      expect(frame.attributes('sandbox')).toBe('allow-scripts');
      await report(900);
      expect(frame.element.style.height).toBe('900px');
      expect(Number.parseFloat(canvas.style.height)).toBeCloseTo(240);
      expect(Number.parseFloat(canvas.style.width)).toBeCloseTo((1024 * 240) / 900);
      await report(200);
      expect(Number.parseFloat(canvas.style.width)).toBeCloseTo(600);
      expect(Number.parseFloat(canvas.style.height)).toBeCloseTo((200 * 600) / 1024);
      await report(1200, window);
      await report(1200, frame.element.contentWindow, 'old-preview');
      await report(Number.POSITIVE_INFINITY);
      await report(-1);
      expect(frame.element.style.height).toBe('200px');
    } finally {
      wrapper.unmount();
      width.mockRestore();
      height.mockRestore();
    }
  });

  it('从小预览打开应用内弹层，关闭后可查看原始代码', async () => {
    const content = '<html><body>完整作品</body></html>';
    const wrapper = mount(IqOutputCode, {
      props: { language: 'html', content },
      attachTo: document.body,
      global: { plugins: [AntdvNext] },
    });
    try {
      expect(wrapper.find('iframe').exists()).toBe(true);
      await wrapper.get('[aria-label="放大作品预览"]').trigger('click');
      await flushPromises();
      const dialog = document.querySelector('[role="dialog"]')!;
      expect(dialog.textContent).toContain('作品预览');
      expect(dialog.querySelector('iframe')?.getAttribute('tabindex')).toBe('0');
      (dialog.querySelector('[aria-label="关闭作品预览"]') as HTMLButtonElement).click();
      await flushPromises();
      expect(wrapper.findAllComponents(IqOutputPreview)).toHaveLength(1);
      await wrapper
        .findAll('button')
        .find((button) => button.text() === '查看代码')!
        .trigger('click');
      expect(wrapper.get('pre code').text()).toBe(content);
    } finally {
      wrapper.unmount();
    }
  });
});
