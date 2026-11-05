export type OperationSource = 'system' | 'user';
export type OperationResult = 'success' | 'failure' | 'cancelled' | 'blocked';

/** 面向列表展示和筛选的稳定动作分类，原始事件名仍保留在 action 中供详情使用。 */
export type OperationActionGroup =
  | 'probe'
  | 'create'
  | 'update'
  | 'delete'
  | 'sync'
  | 'cleanup'
  | 'notification'
  | 'authorization'
  | 'export'
  | 'startup'
  | 'read'
  | 'status_change';

export type OperationAction =
  | 'automatic_probe'
  | 'model_failure'
  | 'model_recovered'
  | 'automatic_notification'
  | 'scheduled_record_cleanup'
  | 'scheduled_log_cleanup'
  | 'application_ready'
  | 'application_blocked'
  | 'site_created'
  | 'site_updated'
  | 'site_deleted'
  | 'site_reordered'
  | 'site_status_changed'
  | 'model_created'
  | 'model_deleted'
  | 'model_status_changed'
  | 'models_synchronized'
  | 'probe_config_saved'
  | 'test_draft_saved'
  | 'manual_probe'
  | 'general_settings_updated'
  | 'notification_settings_updated'
  | 'notification_template_updated'
  | 'test_notification'
  | 'notification_permission_requested'
  | 'record_deleted'
  | 'records_cleared'
  | 'logs_cleared'
  | 'diagnostics_exported'
  | 'page_load_failed'
  | 'detail_read_failed';

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

export const operationActionGroupByAction: Record<OperationAction, OperationActionGroup> = {
  automatic_probe: 'probe',
  model_failure: 'status_change',
  model_recovered: 'status_change',
  automatic_notification: 'notification',
  scheduled_record_cleanup: 'cleanup',
  scheduled_log_cleanup: 'cleanup',
  application_ready: 'startup',
  application_blocked: 'startup',
  site_created: 'create',
  site_updated: 'update',
  site_deleted: 'delete',
  site_reordered: 'update',
  site_status_changed: 'update',
  model_created: 'create',
  model_deleted: 'delete',
  model_status_changed: 'update',
  models_synchronized: 'sync',
  probe_config_saved: 'update',
  test_draft_saved: 'update',
  manual_probe: 'probe',
  general_settings_updated: 'update',
  notification_settings_updated: 'update',
  notification_template_updated: 'update',
  test_notification: 'notification',
  notification_permission_requested: 'authorization',
  record_deleted: 'delete',
  records_cleared: 'cleanup',
  logs_cleared: 'cleanup',
  diagnostics_exported: 'export',
  page_load_failed: 'read',
  detail_read_failed: 'read',
};

export type OperationLogDetailBehavior =
  | { kind: 'drawer' }
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
  items: OperationLogListItem[];
  total: number;
  page: number;
  pageSize: number;
}

export type OperationLogDetailContent =
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
