/**
 * 消息通知领域/DTO 类型。
 *
 * 顶部是「列表页加载契约」：进入页面即取「总开关 + 是否已授权 + 规则摘要列表」。
 * 规则详情按 id 另取（NotificationTemplateDetail = 列表项 + 模板内容）；
 * 标题标记、变量目录属于公共静态配置，走 api/catalog（NotificationMarker / NotificationVariable）。
 * 以 mock 为事实源，后端下一轮按本模型对齐。
 */

/** 模板类型：沿用默认模板，或用户自定义。 */
export type NotificationTemplateMode = 'default' | 'custom';

/** 列表页单条规则摘要（无分页/筛选；详情按 id 查询）。 */
export interface NotificationRuleSummary {
  /** 唯一标识，用于查询对应详情 */
  id: string;
  /** 事件名，如「模型探测故障」 */
  title: string;
  /** 事件描述，如「模型达到稳定故障判定时发送系统通知」 */
  subTitle: string;
  /** 是否启用 */
  status: boolean;
  /** 当前模板类型：默认模板 / 自定义模板 */
  templateType: NotificationTemplateMode;
  /** 投递渠道显示名，如「系统通知」（将来可为 webhook / Telegram 等） */
  channel: string;
}

/** 系统通知授权状态（用枚举而非布尔，便于扩展 未请求 / 被拒 / 不支持 等态）。 */
export type NotificationPermission =
  'unknown' | 'not_requested' | 'denied' | 'granted' | 'unsupported';

/** 进入页面即加载的通知设置。 */
export interface NotificationSettings {
  /** 总开关：是否启用消息通知 */
  notificationsEnabled: boolean;
  /** 系统通知授权状态 */
  authorization: NotificationPermission;
  /** 通知规则摘要列表 */
  rules: NotificationRuleSummary[];
}

/** 发送测试通知的内容。系统通知行用固定样例，模板抽屉用当前草稿——同一接口、不同入参。 */
export interface NotificationTestTemplate {
  title: string;
  body: string;
  markerId?: string;
}
// —— 公共目录类型（由 api/catalog 获取：静态、不入库、跨特性复用）——

/** 标题标记的语义色调（可用于样式区分）。 */
export type NotificationMarkerTone = 'critical' | 'warning' | 'positive' | 'info' | 'neutral';

/**
 * 标题标记目录项：标题前缀的小图标（⚠️/✅/…）。
 * 由后端静态配置、api/catalog 统一提供；规则只用 markerId 引用它。
 */
export interface NotificationMarker {
  /** 标记唯一标识，被 detail.markerId / defaultMarkerId 引用 */
  markerId: string;
  /** 展示符号（emoji 等；空串表示「无标记」） */
  symbol: string;
  /** 可读名称（下拉展示 + 无障碍） */
  name: string;
  /** 语义色调，可选 */
  tone?: NotificationMarkerTone;
}

/**
 * 模板变量目录项：{{key}} 的含义与预览示例值。
 * 由 api/catalog 获取，供「插入变量」菜单与预览取数。
 */
export interface NotificationVariable {
  /** 变量键，如 model / endpoint / time */
  key: string;
  /** 可读名称 */
  label: string;
  /** 说明 */
  description: string;
  /** 预览用示例值 */
  example: string;
  /** 分组，如「通用变量」/「事件变量」 */
  group: string;
}
// —— 规则模板详情（按 id 查询，供模板抽屉）——

/**
 * 规则模板详情 = 列表项（NotificationRuleSummary）+ 这条规则的模板内容。
 * 标题标记目录、变量目录都不在这里，分别走 api/catalog；预览在前端组装。
 */
export interface NotificationTemplateDetail extends NotificationRuleSummary {
  /**
   * 当前选中的「标题标记」id —— 引用 api/catalog 的 markers 目录（见 NotificationMarker.markerId）。
   * 它不是又一个身份 ID，而是“这条规则选了哪个标题图标”的引用。
   */
  markerId: string;
  /** 标题模板（含 {{变量}}），如 "{{model}} 探测故障" */
  titleTemplate: string;
  /** 通知详情模板（含 {{变量}}） */
  bodyTemplate: string;
  /** 该事件默认的标题标记 id（供「恢复默认」/默认模式展示；后续会落到表里） */
  defaultMarkerId: string;
  /** 该事件默认的标题模板 */
  defaultTitleTemplate: string;
  /** 该事件默认的通知详情模板 */
  defaultBodyTemplate: string;
}

/** 保存模板的入参：模板类型 + 自定义时的标记/标题/正文。 */
export interface NotificationTemplateUpdate {
  templateType: NotificationTemplateMode;
  markerId: string;
  titleTemplate: string;
  bodyTemplate: string;
}
