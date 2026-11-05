import type { OperationAction, OperationLogDetailContent } from '@/types/domain/operation-logs';

type Context =
  | 'site'
  | 'model'
  | 'connection'
  | 'policy'
  | 'request'
  | 'parameter'
  | 'general'
  | 'notification'
  | 'rule'
  | 'order';
interface FieldDescription {
  label: string;
  context?: Context;
  nameField?: string;
  values?: Record<string, string>;
  unit?: string;
}

const enabled = { true: '已启用', false: '已停用' };
const templateType = { default: '默认', custom: '自定义' };
const protocols = {
  'openai-compatible': 'OpenAI 兼容协议',
  anthropic: 'Anthropic 协议',
  'openai-responses': 'OpenAI Responses',
  'custom-http': '自定义 HTTP',
};
const permission = {
  unknown: '状态未知',
  not_requested: '未请求授权',
  granted: '已授权',
  denied: '已拒绝',
  unsupported: '不支持',
};
const fields: Record<Context, Record<string, FieldDescription>> = {
  site: {
    id: { label: '站点标识' },
    name: { label: '站点名称' },
    protocol: { label: '接口协议', values: protocols },
    baseUrl: { label: 'Base URL' },
    凭据: { label: '访问凭据' },
    协议: { label: '接口协议', values: protocols },
    启用: { label: '小面板显示', values: { true: '显示', false: '隐藏' } },
    enabled: { label: '小面板显示', values: { true: '显示', false: '隐藏' } },
    credentialConfigured: { label: '访问凭据', values: { true: '已配置', false: '未配置' } },
    connection: { label: '连接配置', context: 'connection' },
    policy: { label: '站点默认探针设置', context: 'policy' },
    models: { label: '模型', context: 'model', nameField: 'name', values: { '[]': '暂无模型' } },
    modelId: { label: '模型标识' },
    customRequest: { label: '自定义探针请求', context: 'request', values: { null: '未启用' } },
    draft: { label: '探针设置草稿', context: 'request' },
    added: { label: '新增模型' },
    removed: { label: '移除模型', context: 'model', nameField: 'name' },
    ids: { label: '站点顺序' },
    顺序: { label: '站点顺序', context: 'order', nameField: 'name' },
  },
  model: {
    id: { label: '模型标识' },
    name: { label: '模型名称' },
    enabled: { label: '探测状态', values: enabled },
    policy: { label: '独立策略', context: 'policy', values: { null: '继承站点默认' } },
    customRequest: { label: '自定义探针请求', context: 'request', values: { null: '未启用' } },
  },
  connection: {
    modelsPath: { label: '模型列表端点' },
    probePath: { label: '探测端点' },
    intervalSeconds: { label: '探测频率', unit: '秒' },
    slowFirstTokenSeconds: { label: '首 Token 慢阈值', unit: '秒' },
    slowTotalSeconds: { label: '总耗时慢阈值', unit: '秒' },
    failureThreshold: { label: '连续失败转红', unit: '次' },
    recoveryThreshold: { label: '连续成功恢复', unit: '次' },
    headers: { label: '请求头' },
  },
  policy: {
    version: { label: '参数结构版本' },
    parameters: {
      label: '请求参数',
      context: 'parameter',
      nameField: 'key',
      values: { '[]': '暂无请求参数' },
    },
  },
  request: {
    version: { label: '参数结构版本' },
    endpoint: { label: '自定义探测端点' },
    parameters: {
      label: '请求参数',
      context: 'parameter',
      nameField: 'key',
      values: { '[]': '暂无请求参数' },
    },
  },
  parameter: {
    key: { label: '字段' },
    value: { label: '值' },
    type: { label: '类型', values: { json: 'JSON' } },
    enabled: { label: '启用', values: enabled },
    binding: { label: '值来源', values: { model: '当前探测模型', null: '手动填写' } },
  },
  general: {
    launchAtStartup: { label: '开机自动启动', values: enabled },
    continueInBackground: { label: '关闭主窗口后继续运行', values: enabled },
    showTrayIcon: { label: '显示状态栏图标', values: { true: '显示', false: '隐藏' } },
    theme: { label: '主题', values: { system: '跟随系统', light: '浅色', dark: '深色' } },
    language: { label: '界面语言', values: { 'zh-CN': '简体中文' } },
    historyRetentionDays: { label: '探测记录保留时间', unit: '天' },
  },
  notification: {
    notificationsEnabled: { label: '启用消息通知', values: enabled },
    authorization: { label: '系统通知授权', values: permission },
    rules: { label: '通知事件', context: 'rule', nameField: 'title' },
    id: { label: '事件标识' },
    title: { label: '事件' },
    subTitle: { label: '事件描述' },
    status: { label: '通知状态', values: enabled },
    templateType: { label: '模板类型', values: templateType },
    channel: { label: '通知渠道' },
    markerId: { label: '标题标记' },
    titleTemplate: { label: '标题' },
    bodyTemplate: { label: '通知详情' },
    defaultMarkerId: { label: '默认标题标记' },
    defaultTitleTemplate: { label: '默认标题' },
    defaultBodyTemplate: { label: '默认通知详情' },
  },
  rule: {
    id: { label: '事件标识' },
    title: { label: '事件' },
    subTitle: { label: '事件描述' },
    status: { label: '通知状态', values: enabled },
    templateType: { label: '模板类型', values: templateType },
    channel: { label: '通知渠道' },
  },
  order: { id: { label: '站点标识' }, name: { label: '站点名称' } },
};

function rootContext(action: OperationAction): Context {
  if (action === 'general_settings_updated') return 'general';
  if (action.startsWith('notification_')) return 'notification';
  if (action === 'model_deleted') return 'model';
  return 'site';
}

/** 只解释已知配置字段；参数原文、模板正文及未知字段不做猜测性翻译。 */
function describeField(
  path: string,
  action: OperationAction,
  names: Map<string, string>,
): FieldDescription {
  if (!path && action === 'notification_permission_requested') {
    return { label: '授权结果', values: permission };
  }
  let context = rootContext(action);
  let prefix = '';
  const labels: string[] = [];
  let description: FieldDescription = { label: path };
  const segments = path.split('.');
  for (const [position, segment] of segments.entries()) {
    if (segment === '提交内容' || segment === '保存结果') {
      labels.push(segment);
      context = segment === '提交内容' && action === 'model_created' ? 'model' : 'site';
    } else {
      const match = /^(.+?)(?:\[(\d+)\])?$/u.exec(segment);
      const entry =
        match &&
        Object.prototype.hasOwnProperty.call(fields[context], match[1]!) &&
        fields[context][match[1]!];
      if (!entry) {
        // 未知后缀属于原始数据，不能继续套用其他层级的字段词典。
        labels.push(segments.slice(position).join('.'));
        return { label: labels.join(' · ') };
      }
      description = entry;
      let label = entry.label;
      if (match[2] !== undefined) {
        label += ` ${Number(match[2]) + 1}`;
        const name = entry.nameField && names.get(`${prefix}${segment}.${entry.nameField}`);
        if (name) label += `「${name}」`;
      }
      labels.push(label);
      if (position < segments.length - 1 && !entry.context) {
        labels.push(segments.slice(position + 1).join('.'));
        return { label: labels.join(' · ') };
      }
      if (entry.context) context = entry.context;
    }
    prefix += `${segment}.`;
  }
  return { ...description, label: labels.join(' · ') };
}

function displayValue(
  value: string,
  field: FieldDescription,
  sensitive: boolean | undefined,
  path: string,
  values: Map<string, string>,
): string {
  if (sensitive) return ['未配置', '已配置', '已更换'].includes(value) ? value : '已隐藏';
  if (value === 'null' && field.values?.null === '继承站点默认') {
    // 自定义请求生效时，空独立策略并不表示正在使用站点默认请求。
    const endpoint = values.get(path.replace(/policy$/u, 'customRequest.endpoint'));
    if (endpoint && endpoint !== '未配置') return '未配置（使用自定义探针请求）';
  }
  if (field.values && Object.prototype.hasOwnProperty.call(field.values, value))
    return field.values[value]!;
  if (field.unit && /^\d+(?:\.\d+)?$/u.test(value)) return `${value} ${field.unit}`;
  return value;
}

/** 展示映射留在操作记录功能内，不修改后端事实、字段契约或持久化内容。 */
export function presentOperationContent(
  content: OperationLogDetailContent,
  action: OperationAction,
): OperationLogDetailContent {
  const names = new Map<string, string>();
  const remember = (path: string, value: string, sensitive?: boolean) => {
    if (sensitive || !value || ['未配置', 'null', '[]'].includes(value)) return;
    const previous = names.get(path);
    names.set(path, previous && previous !== value ? `${previous} → ${value}` : value);
  };
  if (content.kind === 'changes') {
    // 名称仅来自同一次历史事件，不能读取当前站点配置来补全历史。
    for (const change of content.changes) {
      remember(change.field, change.before, change.sensitive);
      remember(change.field, change.after, change.sensitive);
    }
    const before = new Map(content.changes.map((change) => [change.field, change.before]));
    const after = new Map(content.changes.map((change) => [change.field, change.after]));
    return {
      ...content,
      changes: content.changes.map((change) => {
        const field = describeField(change.field, action, names);
        return {
          ...change,
          field: field.label,
          before: displayValue(change.before, field, change.sensitive, change.field, before),
          after: displayValue(change.after, field, change.sensitive, change.field, after),
        };
      }),
    };
  }
  if (content.kind === 'snapshot' || content.kind === 'deletion') {
    for (const field of content.fields) {
      remember(field.label, field.value, 'sensitive' in field && field.sensitive);
    }
    const values = new Map(content.fields.map((field) => [field.label, field.value]));
    return {
      ...content,
      fields: content.fields.map((field) => {
        const description = describeField(field.label, action, names);
        return {
          ...field,
          label: description.label,
          value: displayValue(
            field.value,
            description,
            'sensitive' in field && field.sensitive,
            field.label,
            values,
          ),
        };
      }),
    };
  }
  return content;
}
