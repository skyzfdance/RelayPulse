/**
 * 通知模板的纯逻辑：预览渲染 + 校验 + 书写符号计数。
 * 无 Vue / antd 依赖，便于单测。标记与变量目录由调用方（抽屉）从 api/catalog 取好后传入。
 */
import type { NotificationMarker, NotificationVariable } from '@/types/domain/notifications';

interface SegmenterCtor {
  new (
    locale?: string,
    options?: { granularity: 'grapheme' },
  ): { segment(input: string): Iterable<unknown> };
}

/** 按书写符号计数（emoji / 组合字符算 1），不支持 Segmenter 时回落码点数。 */
export function graphemeCount(value: string): number {
  const Segmenter = (Intl as unknown as { Segmenter?: SegmenterCtor }).Segmenter;
  if (!Segmenter) return Array.from(value).length;
  return Array.from(new Segmenter('zh-CN', { granularity: 'grapheme' }).segment(value)).length;
}

const PLACEHOLDER = /\{\{([a-z][a-z0-9_]*)\}\}/gu;

/** 用示例变量渲染标题/正文预览（标题前拼接标记符号）。 */
export function renderTemplatePreview(
  titleTemplate: string,
  bodyTemplate: string,
  variables: NotificationVariable[],
  marker: NotificationMarker | undefined,
): { title: string; body: string } {
  const examples = new Map(variables.map((item) => [item.key, item.example] as const));
  const render = (template: string) =>
    template.replace(PLACEHOLDER, (_, key: string) => examples.get(key) ?? '');
  const title = render(titleTemplate);
  return {
    title: marker?.symbol ? `${marker.symbol} ${title}` : title,
    body: render(bodyTemplate),
  };
}

export interface TemplateValidation {
  title?: string;
  body?: string;
  marker?: string;
}

function validateText(value: string, maximum: number, allowNewlines: boolean): string | undefined {
  // 与 Rust str::trim 的 Unicode White_Space 语义一致。
  if (/^\p{White_Space}*$/u.test(value)) return '内容不能为空';
  if (graphemeCount(value) > maximum) return `不能超过 ${maximum} 个字符`;
  const hasControl = Array.from(value).some(
    (character) => /\p{Cc}/u.test(character) && !(allowNewlines && /[\r\n]/u.test(character)),
  );
  if (hasControl || (!allowNewlines && /[\u2028\u2029]/u.test(value)))
    return '不能包含控制字符或换行';
  return undefined;
}

function validateField(
  value: string,
  maximum: number,
  allowNewlines: boolean,
  allowed: Set<string>,
): string | undefined {
  const error = validateText(value, maximum, allowNewlines);
  if (error) return error;
  for (const match of value.matchAll(PLACEHOLDER)) {
    if (!allowed.has(match[1])) return `变量 {{${match[1]}}} 不适用于当前事件`;
  }
  if (value.replace(PLACEHOLDER, '').match(/[{}]/u)) return '变量占位符格式不完整';
  return undefined;
}

/** 校验标题/正文/标记：变量白名单 + 字数 + 控制字符 + 标记可用性。 */
export function validateTemplate(
  titleTemplate: string,
  bodyTemplate: string,
  variables: NotificationVariable[],
  marker: NotificationMarker | undefined,
): TemplateValidation {
  const allowed = new Set(variables.map((item) => item.key));
  const title = validateField(titleTemplate, 120, false, allowed);
  const body = validateField(bodyTemplate, 500, true, allowed);
  if (title || body || !marker) {
    return { title, body, marker: marker ? undefined : '标题标记不可用' };
  }
  const preview = renderTemplatePreview(titleTemplate, bodyTemplate, variables, marker);
  return {
    title:
      graphemeCount(preview.title) > 120
        ? '标记和标题合计不能超过 120 个字符'
        : validateText(preview.title, 120, false),
  };
}

/** 测试发送校验成品文字，不把成品里的花括号重新解释为模板变量。 */
export function validateNotificationPreview(preview: {
  title: string;
  body: string;
}): Pick<TemplateValidation, 'title' | 'body'> {
  return {
    title: validateText(preview.title, 120, false),
    body: validateText(preview.body, 500, true),
  };
}
