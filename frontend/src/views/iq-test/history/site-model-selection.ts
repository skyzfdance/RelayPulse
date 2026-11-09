import type { DirectiveBinding, ObjectDirective } from 'vue';

interface SiteModelSelection {
  siteId: string;
  paths: string[][];
}

function updateSiteSelection(element: HTMLElement, binding: DirectiveBinding<SiteModelSelection>) {
  const item = element.closest<HTMLElement>('[role="menuitemcheckbox"]');
  const checkbox = item?.querySelector<HTMLElement>(':scope > .ant-cascader-checkbox');
  if (!item || !checkbox) return;
  const { siteId, paths } = binding.value;
  const checked = paths.some((path) => path[0] === siteId && path.length === 1);
  const partial = !checked && paths.some((path) => path[0] === siteId && path.length > 1);

  // Cascader 的独立勾选不生成半选集合，仅补齐原生勾选框的外观与读屏状态。
  // 依赖 antdv 的原生 checkbox 类名，不更改选择值或接管鼠标、键盘事件。
  checkbox.classList.toggle('ant-cascader-checkbox-indeterminate', partial);
  item.setAttribute('aria-checked', partial ? 'mixed' : String(checked));
}

export const vSiteModelSelection: ObjectDirective<HTMLElement, SiteModelSelection> = {
  mounted: updateSiteSelection,
  updated: updateSiteSelection,
};
