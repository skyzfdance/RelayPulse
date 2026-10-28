/**
 * v-loading：在绑定节点内部注入一个局部 a-spin 遮罩（相对定位覆盖该节点），
 * 用法贴近 Element Plus 的 v-loading——不必手动用 <a-spin> 包裹内容。
 *
 * 绑定值：
 * - 布尔：`v-loading="spinning"`；
 * - 对象：`v-loading="{ loading: spinning, tip: '数据加载中...' }"`（tip 可选，不传则只有转圈）。
 *
 * 指示器用 antd 默认 Spin（非 fullscreen，小圆点本就是主色，随明暗主题）。
 * 遮罩节点经命令式 render 挂进宿主元素，故宿主需非 static 定位——static 时自动补 relative。
 */
import { Spin } from 'antdv-next';
import { type Directive, h, render } from 'vue';

type LoadingValue = boolean | { loading?: boolean; tip?: string };

interface LoadingController {
  container: HTMLElement;
  mounted: boolean;
  update(loading: boolean, tip?: string): void;
  destroy(): void;
}

const CONTROLLER = Symbol('rp-loading');

type LoadingHost = HTMLElement & { [CONTROLLER]?: LoadingController };

function normalize(value: LoadingValue | undefined): { loading: boolean; tip?: string } {
  if (typeof value === 'boolean') return { loading: value };
  return { loading: Boolean(value?.loading), tip: value?.tip };
}

function ensureController(el: LoadingHost): LoadingController {
  const existing = el[CONTROLLER];
  if (existing) return existing;

  const container = document.createElement('div');
  container.className = 'rp-loading-mask';

  const controller: LoadingController = {
    container,
    mounted: false,
    update(loading, tip) {
      if (loading) {
        const position = getComputedStyle(el).position;
        if (!position || position === 'static') el.style.position = 'relative';
        if (!this.mounted) {
          el.appendChild(container);
          this.mounted = true;
        }
        render(
          h('div', { class: 'rp-loading-mask__inner' }, [
            h(Spin, { spinning: true }),
            tip ? h('div', { class: 'rp-loading-mask__tip' }, tip) : null,
          ]),
          container,
        );
      } else if (this.mounted) {
        render(null, container);
        el.removeChild(container);
        this.mounted = false;
      }
    },
    destroy() {
      render(null, container);
      if (this.mounted && container.parentNode) container.parentNode.removeChild(container);
      this.mounted = false;
    },
  };

  el[CONTROLLER] = controller;
  return controller;
}

export const vLoading: Directive<LoadingHost, LoadingValue> = {
  mounted(el, binding) {
    const { loading, tip } = normalize(binding.value);
    ensureController(el).update(loading, tip);
  },
  updated(el, binding) {
    const { loading, tip } = normalize(binding.value);
    ensureController(el).update(loading, tip);
  },
  unmounted(el) {
    el[CONTROLLER]?.destroy();
    delete el[CONTROLLER];
  },
};
