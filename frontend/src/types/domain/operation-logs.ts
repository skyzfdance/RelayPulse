export type OperationSource = 'system' | 'user';
export type OperationResult = 'success' | 'failure' | 'cancelled' | 'blocked';

/** 面向列表展示和筛选的稳定动作分类，原始事件名仍保留在 action 中供详情使用。 */
export type OperationActionGroup = string;
export type OperationAction = string;

export const operationActionGroupLabels: Record<OperationActionGroup, string> = {
  probe: '探测',
  create: '新增',
  update: '修改',
  delete: '删除',
  sync: '同步',
  cleanup: '清理',
  notification: '通知',
  authorization: '授权',
  export: '导出',
  startup: '启动',
  read: '读取',
  status_change: '状态变化',
};

export type OperationLogDetailBehavior =
  | { kind: 'drawer' }
  | { kind: 'navigate'; target: string; parameters: Record<string, string> }
  | { kind: 'probe_records'; recordIds: string[]; availability: 'available' | 'expired' | 'none' };

export interface OperationLogListItem {
  id: string;
  occurredAt: string;
  source: OperationSource;
  actorSnapshot: string;
  action: OperationAction;
  actionGroup: OperationActionGroup;
  actionLabel: string;
  objectSnapshot: string;
  result: OperationResult;
  summary: string;
  detailBehavior: OperationLogDetailBehavior;
}

export interface OperationLogQuery {
  startDate: string | null;
  endDate: string | null;
  source: OperationSource[];
  result: OperationResult[];
  actions: OperationActionGroup[];
  page: number;
  pageSize: number;
}

export interface OperationLogPage {
  actionGroups?: string[];
  items: OperationLogListItem[];
  total: number;
  page: number;
  pageSize: number;
}

export type OperationLogDetailContent =
  | {
      kind: 'presentation';
      format: string;
      fields: { label: string; value: string; sensitive?: boolean }[];
    }
  | { kind: 'snapshot'; fields: { label: string; value: string; sensitive?: boolean }[] }
  | {
      kind: 'changes';
      changes: { field: string; before: string; after: string; sensitive?: boolean }[];
    }
  | { kind: 'deletion'; summary: string; fields: { label: string; value: string }[] }
  | { kind: 'notification'; trigger: string; title: string; body: string; sendResult: string }
  | { kind: 'cleanup'; target: string; scope: string; count: number; space?: string }
  | { kind: 'export'; filename: string; fileSize: string; outcome: string }
  | { kind: 'failure'; activity: string; target: string; reason: string }
  | { kind: 'probe_failure'; reason: string };

export interface OperationLogDetail {
  item: OperationLogListItem;
  explanation: string;
  content: OperationLogDetailContent;
}

/** 前端独有读取失败可上报的固定业务目标，不接受任意事件或对象正文。 */
export type ReadFailureTarget =
  | 'site_list'
  | 'site_detail'
  | 'probe_records'
  | 'probe_record_detail'
  | 'notification_settings'
  | 'notification_template'
  | 'general_settings'
  | 'panel';
