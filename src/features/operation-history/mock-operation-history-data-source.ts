import type {
  OperationHistoryDataSource,
  OperationHistoryPageResult,
  OperationHistoryQuery,
  OperationRecordDetails,
  OperationRecordSummary,
} from './operation-history-types';
import { OperationHistoryError } from './operation-history-types';

export type OperationHistoryScenario = 'default' | 'empty' | 'error' | 'loading' | 'cursor-expired';

const details: OperationRecordDetails[] = [
  {
    recordRef: 'record-config-site-1',
    occurredAt: { value: '2026-08-08T15:36:42+08:00', precision: 'exact' },
    category: 'configuration_change',
    trigger: 'user_action',
    actionLabel: '调整站点配置',
    description: '用户修改了 OpenAI 主线路的站点配置，保存成功。',
    subject: 'OpenAI 主线路',
    detail: {
      kind: 'configuration_change',
      changes: [
        { fieldLabel: '站点名称', before: 'OpenAI', after: 'OpenAI 主线路' },
        { fieldLabel: '启用状态', before: '已关闭', after: '已开启' },
        { fieldLabel: '超时时间', before: '15 秒', after: '30 秒' },
        { fieldLabel: 'API Key', before: '已设置', after: '已更新' },
      ],
    },
  },
  {
    recordRef: 'record-collection-auto-1',
    occurredAt: { value: '2026-08-08T15:20:10+08:00', precision: 'exact' },
    category: 'data_collection',
    trigger: 'automatic',
    actionLabel: '完成自动数据采集',
    description: 'RelayPulse 已完成本轮自动数据采集。',
    detail: {
      kind: 'data_collection',
      siteCount: 3,
      modelCount: 13,
      completedCount: 12,
      failedCount: 1,
      skippedCount: 0,
      canOpenProbeRecords: true,
    },
  },
  {
    recordRef: 'record-startup-1',
    occurredAt: { value: '2026-08-08T14:58:06+08:00', precision: 'exact' },
    category: 'app_lifecycle',
    trigger: 'system_startup',
    actionLabel: '启动 RelayPulse',
    description: 'RelayPulse 已启动，并按用户配置恢复后台任务。',
    subject: 'RelayPulse',
    detail: {
      kind: 'app_lifecycle',
      event: 'started',
      launchMethod: '用户打开',
      appVersion: '0.1.0',
      restoredSiteCount: 4,
      restoredModelCount: 14,
      backgroundTasksStarted: true,
    },
  },
  {
    recordRef: 'record-export-1',
    occurredAt: { value: '2026-08-08T11:42:19+08:00', precision: 'exact' },
    category: 'diagnostics_maintenance',
    trigger: 'user_action',
    actionLabel: '导出诊断包',
    description: '用户导出了经过脱敏处理的诊断包。',
    detail: {
      kind: 'diagnostics_maintenance',
      action: 'export',
      includedCategories: ['应用配置摘要', '运行环境摘要', '结构化诊断日志'],
      fileCount: 4,
      bundleSizeLabel: '1.8 MB',
      locationLabel: '用户选择的位置',
      requestBodiesExcluded: true,
    },
  },
  {
    recordRef: 'record-abnormal-exit-1',
    occurredAt: {
      value: '2026-08-08T09:12:00+08:00',
      precision: 'approximate',
      qualifier: 'after',
    },
    category: 'app_lifecycle',
    trigger: 'system_recovery',
    actionLabel: '检测到上次异常退出',
    description: '本次启动时发现上一次会话缺少正常退出标记。',
    subject: 'RelayPulse',
    detail: {
      kind: 'app_lifecycle',
      event: 'abnormal_exit',
      previousSessionStartedAt: '2026-08-08T08:46:31+08:00',
      lastCompletedAction: '完成自动数据采集',
      failureSource: 'Rust 运行时',
      crashReportFound: false,
    },
    exception: {
      reason: '上一次会话未记录正常退出。',
      safeCode: 'previous_session_incomplete',
      lastStep: '完成自动数据采集',
      recommendedAction: '如问题再次发生，请导出诊断包。',
    },
  },
  {
    recordRef: 'record-model-failed-1',
    occurredAt: { value: '2026-08-07T21:34:55+08:00', precision: 'exact' },
    category: 'configuration_change',
    trigger: 'user_action',
    actionLabel: '新增模型失败',
    description: '用户尝试新增模型，但配置未能保存。',
    subject: 'OpenAI 主线路',
    detail: {
      kind: 'configuration_change',
      changes: [{ fieldLabel: '模型名称' }, { fieldLabel: '启用状态' }],
    },
    exception: {
      reason: '模型名称与当前站点中的已有模型重复。',
      safeCode: 'model_name_conflict',
      lastStep: '校验模型配置',
      recommendedAction: '返回站点管理并使用其他模型名称。',
    },
  },
  {
    recordRef: 'record-cleanup-1',
    occurredAt: { value: '2026-08-07T18:22:18+08:00', precision: 'exact' },
    category: 'diagnostics_maintenance',
    trigger: 'user_action',
    actionLabel: '清理诊断日志',
    description: '用户清理了本机诊断日志，站点、模型和应用设置未受影响。',
    detail: {
      kind: 'diagnostics_maintenance',
      action: 'cleanup',
      beforeSizeLabel: '9.7 MB',
      fileCount: 6,
      afterSizeLabel: '192 B',
      settingsPreserved: true,
    },
  },
  {
    recordRef: 'record-exit-1',
    occurredAt: { value: '2026-08-07T17:48:03+08:00', precision: 'exact' },
    category: 'app_lifecycle',
    trigger: 'system_shutdown',
    actionLabel: '正常退出 RelayPulse',
    description: 'RelayPulse 已正常退出并保存窗口与后台状态。',
    subject: 'RelayPulse',
    detail: {
      kind: 'app_lifecycle',
      event: 'exited',
      settingsSaved: true,
      windowStateSaved: true,
      backgroundTasksStopped: true,
    },
  },
  {
    recordRef: 'record-log-limit-1',
    occurredAt: { value: '2026-08-07T16:06:41+08:00', precision: 'exact' },
    category: 'configuration_change',
    trigger: 'user_action',
    actionLabel: '修改诊断日志容量',
    description: '用户修改了诊断日志总容量，新配置将在下次启动时生效。',
    subject: '诊断日志',
    detail: {
      kind: 'configuration_change',
      changes: [{ fieldLabel: '日志总容量', before: '10 MB', after: '50 MB' }],
    },
  },
  {
    recordRef: 'record-collection-manual-1',
    occurredAt: { value: '2026-08-07T13:28:32+08:00', precision: 'exact' },
    category: 'data_collection',
    trigger: 'user_action',
    actionLabel: '手动数据采集中断',
    description: '用户启动的数据采集在处理过程中异常中断。',
    detail: {
      kind: 'data_collection',
      siteCount: 3,
      modelCount: 13,
      completedCount: 7,
      failedCount: 1,
      skippedCount: 5,
      canOpenProbeRecords: true,
    },
    exception: {
      reason: '后台采集任务未能完成。',
      safeCode: 'collection_interrupted',
      lastStep: '处理第 8 个模型',
      recommendedAction: '前往探测记录查看已完成的探测结果。',
    },
  },
  {
    recordRef: 'record-startup-2',
    occurredAt: { value: '2026-08-07T09:03:11+08:00', precision: 'exact' },
    category: 'app_lifecycle',
    trigger: 'system_startup',
    actionLabel: '启动 RelayPulse',
    description: 'RelayPulse 已通过开机启动方式运行。',
    subject: 'RelayPulse',
    detail: {
      kind: 'app_lifecycle',
      event: 'started',
      launchMethod: '开机启动',
      appVersion: '0.1.0',
      restoredSiteCount: 4,
      restoredModelCount: 14,
      backgroundTasksStarted: true,
    },
  },
  {
    recordRef: 'record-delete-site-1',
    occurredAt: { value: '2026-08-06T22:15:47+08:00', precision: 'exact' },
    category: 'configuration_change',
    trigger: 'user_action',
    actionLabel: '删除站点',
    description: '用户删除了已停用的旧代理站点。',
    subject: '旧代理节点',
    detail: {
      kind: 'configuration_change',
      changes: [{ fieldLabel: '站点', before: '旧代理节点', after: '已删除' }],
    },
  },
  {
    recordRef: 'record-export-2',
    occurredAt: { value: '2026-08-06T18:51:26+08:00', precision: 'exact' },
    category: 'diagnostics_maintenance',
    trigger: 'user_action',
    actionLabel: '导出诊断包',
    description: '用户导出了经过脱敏处理的诊断包。',
    detail: {
      kind: 'diagnostics_maintenance',
      action: 'export',
      includedCategories: ['应用配置摘要', '运行环境摘要', '结构化诊断日志'],
      fileCount: 3,
      bundleSizeLabel: '1.2 MB',
      locationLabel: '桌面',
      requestBodiesExcluded: true,
    },
  },
  {
    recordRef: 'record-toggle-site-1',
    occurredAt: { value: '2026-08-06T16:44:09+08:00', precision: 'exact' },
    category: 'configuration_change',
    trigger: 'user_action',
    actionLabel: '停用站点',
    description: '用户停用了本地 Ollama 站点。',
    subject: '本地 Ollama',
    detail: {
      kind: 'configuration_change',
      changes: [{ fieldLabel: '启用状态', before: '已开启', after: '已关闭' }],
    },
  },
];

const summaries: OperationRecordSummary[] = details.map(
  ({ recordRef, occurredAt, category, actionLabel }) => ({
    recordRef,
    occurredAt,
    category,
    actionLabel,
  }),
);

function wait(milliseconds: number) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

export class MockOperationHistoryDataSource implements OperationHistoryDataSource {
  constructor(private readonly scenario: OperationHistoryScenario = 'default') {}

  async listRecords(query: OperationHistoryQuery): Promise<OperationHistoryPageResult> {
    await wait(this.scenario === 'loading' ? 1_200 : 80);
    if (this.scenario === 'error') {
      throw new OperationHistoryError('backend_unavailable', '操作记录查询服务暂时不可用');
    }
    if (this.scenario === 'cursor-expired' && query.cursor) {
      throw new OperationHistoryError('cursor_expired', '日志内容已变化');
    }

    const source = this.scenario === 'empty' ? [] : summaries;
    const filtered = source.filter((record) => {
      const timestamp = new Date(record.occurredAt.value).getTime();
      const after = query.startedAfter ? new Date(query.startedAfter).getTime() : null;
      const before = query.startedBefore ? new Date(query.startedBefore).getTime() : null;
      const matchesQuery = query.operationQuery
        ? record.actionLabel
            .toLocaleLowerCase('zh-CN')
            .includes(query.operationQuery.toLocaleLowerCase('zh-CN'))
        : true;
      return (
        (!query.category || record.category === query.category) &&
        (after === null || timestamp >= after) &&
        (before === null || timestamp <= before) &&
        matchesQuery
      );
    });
    const offset = query.cursor ? Number.parseInt(query.cursor.replace('mock:', ''), 10) : 0;
    const safeOffset = Number.isFinite(offset) ? offset : 0;
    const items = filtered.slice(safeOffset, safeOffset + query.limit);
    const nextOffset = safeOffset + items.length;
    return clone({
      items,
      nextCursor: nextOffset < filtered.length ? `mock:${nextOffset}` : undefined,
      datasetVersion: 'mock-dataset-v1',
    });
  }

  async getRecordDetails(recordRef: string): Promise<OperationRecordDetails> {
    await wait(80);
    const record = details.find((candidate) => candidate.recordRef === recordRef);
    if (!record) throw new OperationHistoryError('not_found', '这条操作记录已不可用');
    return clone(record);
  }
}

export class PendingOperationHistoryDataSource implements OperationHistoryDataSource {
  async listRecords(): Promise<OperationHistoryPageResult> {
    throw new OperationHistoryError('backend_unavailable', '操作记录后端查询接口尚未接入');
  }

  async getRecordDetails(): Promise<OperationRecordDetails> {
    throw new OperationHistoryError('backend_unavailable', '操作记录后端详情接口尚未接入');
  }
}

export function operationHistoryScenarioFromSearch(search: string): OperationHistoryScenario {
  const requested = new URLSearchParams(search).get('operationScenario');
  const scenarios: OperationHistoryScenario[] = [
    'default',
    'empty',
    'error',
    'loading',
    'cursor-expired',
  ];
  return scenarios.includes(requested as OperationHistoryScenario)
    ? (requested as OperationHistoryScenario)
    : 'default';
}
