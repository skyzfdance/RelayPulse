import type { ReadFailureTarget } from '@/types/domain/operation-logs';
import type { Component, ComponentPublicInstance } from 'vue';

import { reportOperationReadFailure } from '@/api/operation-read-failures';
import { useFeedback } from '@/composables/useFeedback';

// 仅登记显式绑定的业务页根组件；布局、详情子组件和全局宿主没有页面归属。
const pageTargets = new WeakMap<Component, ReadFailureTarget>();
const reportedInstances = new WeakSet<ComponentPublicInstance>();

class PageLoadFailure extends Error {
  constructor(
    readonly target: ReadFailureTarget,
    readonly loadCause: unknown,
  ) {
    super('业务页面模块加载失败');
  }
}

export function operationPage(
  target: ReadFailureTarget,
  load: () => Promise<{ default: Component }>,
): () => Promise<Component> {
  return async () => {
    try {
      const { default: page } = await load();
      pageTargets.set(page, target);
      return page;
    } catch (cause) {
      throw new PageLoadFailure(target, cause);
    }
  };
}

/** 路由只传入当前导航的错误；普通守卫异常没有业务读取含义。 */
export function reportOperationPageLoadFailure(cause: unknown): void {
  if (!(cause instanceof PageLoadFailure)) return;
  useFeedback().error('页面暂时无法加载，请重试。', 'message');
  reportOperationReadFailure(cause.target, 'page', cause.loadCause);
}

/** 仅当前路由的根组件 setup/render 失败；不把子组件错误归给页面。 */
export function reportOperationPageRenderFailure(
  activePage: Component | undefined,
  instance: ComponentPublicInstance | null,
  cause: unknown,
  info: string,
): void {
  if (
    !activePage ||
    !instance ||
    instance.$.isUnmounted ||
    instance.$.type !== activePage ||
    reportedInstances.has(instance)
  )
    return;
  const target = pageTargets.get(activePage);
  if (
    !target ||
    ![
      'render function',
      'setup function',
      'https://vuejs.org/error-reference/#runtime-0',
      'https://vuejs.org/error-reference/#runtime-1',
    ].includes(info)
  )
    return;
  reportedInstances.add(instance);
  useFeedback().error('页面内容暂时无法显示，请重新打开页面。', 'message');
  reportOperationReadFailure(target, 'page', cause);
}
