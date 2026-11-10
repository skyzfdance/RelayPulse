import type {
  OperationAction,
  OperationLogDetail,
  OperationLogDetailContent,
  OperationLogListItem,
  OperationLogPage,
  OperationLogQuery,
} from '@/types/domain/operation-logs';

import { RelayPulseBackendError } from '@/utils/backend-error';

import { registerMockHandler } from '../registry';
import { recordsState } from './records';

const operationActionGroupByAction: Record<string, string> = {
  iq_question_created: 'create',
  iq_question_updated: 'update',
  iq_question_deleted: 'delete',
  iq_run_started: 'create',
  iq_run_aborted: 'status_change',
  iq_run_deleted: 'delete',
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

interface Fixture extends Omit<
  OperationLogListItem,
  'id' | 'occurredAt' | 'actorSnapshot' | 'actionGroup'
> {
  actorSnapshot?: string;
  occurredAt?: string;
  content: OperationLogDetailContent;
  explanation: string;
}

const probeLink = (recordIds: string[]): OperationLogListItem['detailBehavior'] => ({
  kind: 'probe_records',
  recordIds,
  availability: 'available',
});

/** 事件在本次正式探测记录均已完成后发生，时间与当时保留的记录快照一致。 */
function afterProbeRecords(recordIds: string[]) {
  const records = recordsState.records.filter((record) => recordIds.includes(record.id));
  const completedAt = records.map(
    (record) => Date.parse(record.probedAt) + (record.durationMs ?? 0),
  );
  return new Date(
    (completedAt.length ? Math.max(...completedAt) : Date.now()) + 1000,
  ).toISOString();
}

const fixtures: Fixture[] = [
  {
    source: 'system',
    action: 'automatic_probe',
    actionLabel: '自动探测',
    objectSnapshot: 'doudui.cc',
    result: 'success',
    summary: '系统完成站点 doudui.cc 的自动探测。',
    detailBehavior: probeLink(['probe-002', 'probe-003', 'probe-004']),
    occurredAt: afterProbeRecords(['probe-002', 'probe-003', 'probe-004']),
    explanation: '系统完成站点 doudui.cc 的自动探测。',
    content: { kind: 'snapshot', fields: [{ label: '任务', value: '定时探测' }] },
  },
  {
    source: 'user',
    action: 'site_updated',
    actionLabel: '修改站点连接配置',
    objectSnapshot: 'ai.input.im',
    result: 'success',
    summary: '用户修改了站点 ai.input.im 的连接地址、连接超时和访问凭据。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存站点 ai.input.im 的连接配置。',
    content: {
      kind: 'changes',
      changes: [
        { field: 'API 地址', before: 'https://old.example/v1', after: 'https://input.codes/v1' },
        { field: '连接超时', before: '30 秒', after: '45 秒' },
        { field: 'API 密钥', before: '已配置', after: '已更换', sensitive: true },
      ],
    },
  },
  {
    source: 'system',
    action: 'model_failure',
    actionLabel: '模型探测异常',
    objectSnapshot: 'GPT-6 Astra · ai.input.im',
    result: 'failure',
    summary: '系统在自动探测中发现站点 ai.input.im 的模型 GPT-6 Astra 连接超时，未收到供应商响应。',
    detailBehavior: { kind: 'drawer' },
    explanation: '自动探测发现 GPT-6 Astra 请求失败。',
    content: {
      kind: 'failure',
      activity: '自动探测模型',
      target: 'GPT-6 Astra · ai.input.im',
      reason: '连接超时，未收到供应商响应。',
    },
  },
  {
    source: 'user',
    action: 'notification_template_updated',
    actionLabel: '修改通知模板',
    objectSnapshot: '模型异常模板',
    result: 'success',
    summary: '用户修改了“模型异常模板”的通知标题和正文。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存通知模板“模型异常模板”。',
    content: {
      kind: 'changes',
      changes: [
        { field: '标题', before: '模型异常', after: '模型 {{model}} 无法连接' },
        { field: '正文', before: '请检查服务', after: '站点 {{site}} 的模型 {{model}} 探测失败。' },
      ],
    },
  },
  {
    source: 'system',
    action: 'automatic_notification',
    actionLabel: '自动发送通知',
    objectSnapshot: '模型异常通知',
    result: 'success',
    summary: '系统因站点 ai.input.im 的模型 GPT-6 Astra 探测异常，自动发送了桌面通知。',
    detailBehavior: { kind: 'drawer' },
    explanation: '系统因模型探测异常发送了通知。',
    content: {
      kind: 'notification',
      trigger: 'GPT-6 Astra 在 ai.input.im 探测失败',
      title: '模型 GPT-6 Astra 无法连接',
      body: [
        '站点 ai.input.im 的模型 GPT-6 Astra 探测失败。本次请求在等待供应商响应时超过连接超时限制，系统尚未收到有效响应。',
        '系统已保留这次失败的探测记录，你可以在探测记录中查看本次请求的时间、结果和已保存的响应信息。当前通知仅说明这次探测发生的情况，后续自动探测仍会按已保存的配置执行。',
        '如果服务持续不可用，请检查站点 API 地址是否正确、凭据是否仍然有效，以及供应商服务是否处于维护状态。修改连接配置后，可以主动执行一次手动探测，以确认更新后的配置能够正常访问模型。',
        '模型恢复后，系统会根据已保存的通知设置发送恢复通知。此通知由模型异常规则自动触发，内容保留的是发送当时的站点和模型名称。',
      ].join('\n\n'),
      sendResult: '已发送',
    },
  },
  {
    source: 'user',
    action: 'site_created',
    actionLabel: '新增站点',
    objectSnapshot: 'doudui.cc',
    result: 'success',
    summary: '用户新增了 OpenAI 兼容站点 doudui.cc。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已新增站点 doudui.cc。',
    content: {
      kind: 'snapshot',
      fields: [
        { label: '站点名称', value: 'doudui.cc' },
        { label: '协议', value: 'OpenAI 兼容协议' },
        { label: 'API 地址', value: 'https://doudui.cc/v1' },
        { label: 'API 密钥', value: '已配置', sensitive: true },
      ],
    },
  },
  {
    source: 'user',
    action: 'model_created',
    actionLabel: '新增模型',
    objectSnapshot: 'GPT-6.1 Sol · doudui.cc',
    result: 'success',
    summary: '用户在站点 doudui.cc 新增了模型 GPT-6.1 Sol。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已在站点 doudui.cc 新增模型 GPT-6.1 Sol。',
    content: {
      kind: 'snapshot',
      fields: [
        { label: '模型名称', value: 'GPT-6.1 Sol' },
        { label: '所属站点', value: 'doudui.cc' },
        { label: '探测状态', value: '已启用' },
      ],
    },
  },
  {
    source: 'user',
    action: 'probe_config_saved',
    actionLabel: '保存探测配置',
    objectSnapshot: 'Anthropic',
    result: 'success',
    summary: '用户修改了站点 Anthropic 的探测间隔和慢响应阈值。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存 Anthropic 的探测配置。',
    content: {
      kind: 'changes',
      changes: [
        { field: '探测间隔', before: '15 分钟', after: '10 分钟' },
        { field: '总耗时阈值', before: '30 秒', after: '45 秒' },
      ],
    },
  },
  {
    source: 'user',
    action: 'manual_probe',
    actionLabel: '手动探测',
    objectSnapshot: 'doudui.cc',
    result: 'success',
    summary: '用户完成站点 doudui.cc 中 GPT-6 Astra 的手动探测。',
    detailBehavior: probeLink(['probe-008']),
    occurredAt: afterProbeRecords(['probe-008']),
    explanation: '用户手动完成站点 doudui.cc 的探测。',
    content: { kind: 'snapshot', fields: [{ label: '触发方式', value: '界面手动发起' }] },
  },
  {
    source: 'user',
    action: 'general_settings_updated',
    actionLabel: '修改通用设置',
    objectSnapshot: '通用设置',
    result: 'success',
    summary: '用户修改了 RelayPulse 的启动和窗口行为设置。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存通用设置。',
    content: {
      kind: 'changes',
      changes: [
        { field: '启动时运行', before: '关闭', after: '开启' },
        { field: '关闭窗口时', before: '退出应用', after: '最小化到托盘' },
      ],
    },
  },
  {
    source: 'user',
    action: 'notification_settings_updated',
    actionLabel: '修改通知设置',
    objectSnapshot: '通知设置',
    result: 'success',
    summary: '用户在通知设置中开启了模型恢复通知。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存通知设置。',
    content: {
      kind: 'changes',
      changes: [{ field: '模型恢复时通知', before: '关闭', after: '开启' }],
    },
  },
  {
    source: 'user',
    action: 'test_notification',
    actionLabel: '测试通知',
    objectSnapshot: '桌面通知',
    result: 'cancelled',
    summary: '用户取消了桌面测试通知，通知未发送。',
    detailBehavior: { kind: 'drawer' },
    explanation: '测试通知已取消，没有发送通知。',
    content: {
      kind: 'notification',
      trigger: '用户主动测试',
      title: 'RelayPulse 测试通知',
      body: '这是一条通知预览内容。',
      sendResult: '已取消，未发送',
    },
  },
  {
    source: 'user',
    action: 'models_synchronized',
    actionLabel: '同步模型列表',
    objectSnapshot: 'Anthropic',
    result: 'failure',
    summary: '用户同步站点 Anthropic 的模型列表失败，服务端返回 HTTP 503，服务暂时不可用。',
    detailBehavior: { kind: 'drawer' },
    explanation: '同步 Anthropic 的模型列表失败。',
    content: {
      kind: 'failure',
      activity: '同步模型列表',
      target: 'Anthropic',
      reason: '服务器返回 HTTP 503：服务暂时不可用。',
    },
  },
  {
    source: 'system',
    action: 'model_recovered',
    actionLabel: '模型恢复正常',
    objectSnapshot: 'Claude Sonnet · Anthropic',
    result: 'success',
    summary: '系统通过自动探测确认站点 Anthropic 的模型 Claude Sonnet 已恢复正常。',
    detailBehavior: { kind: 'drawer' },
    explanation: '自动规则确认 Claude Sonnet 已恢复正常。',
    content: { kind: 'snapshot', fields: [{ label: '触发原因', value: '连续探测成功' }] },
  },
  {
    source: 'user',
    action: 'site_reordered',
    actionLabel: '调整站点顺序',
    objectSnapshot: '站点列表',
    result: 'success',
    summary: '用户调整了 Anthropic、ai.input.im 和 doudui.cc 的站点显示顺序。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已更新站点显示顺序。',
    content: {
      kind: 'changes',
      changes: [
        {
          field: '站点顺序',
          before: 'ai.input.im → Anthropic → doudui.cc',
          after: 'Anthropic → ai.input.im → doudui.cc',
        },
      ],
    },
  },
  {
    source: 'user',
    action: 'record_deleted',
    actionLabel: '删除探测记录',
    objectSnapshot: 'probe-014 · ai.input.im',
    result: 'success',
    summary: '用户删除了站点 ai.input.im 中 GPT-6 Astra 的探测记录 probe-014。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已删除探测记录 probe-014。',
    content: {
      kind: 'deletion',
      summary: '删除前记录了模型、站点、探测时间和本次结果。',
      fields: [
        { label: '站点', value: 'ai.input.im' },
        { label: '模型', value: 'GPT-6 Astra' },
        { label: '结果', value: '响应偏慢' },
      ],
    },
  },
  {
    source: 'system',
    action: 'scheduled_record_cleanup',
    actionLabel: '定时清理探测记录',
    objectSnapshot: '探测记录',
    result: 'success',
    summary: '系统按 30 天保留期限，定时清理了 12 条过期探测记录。',
    detailBehavior: { kind: 'drawer' },
    explanation: '系统按保留期限清理了过期探测记录。',
    content: { kind: 'cleanup', target: '探测记录', scope: '超过 30 天的记录', count: 12 },
  },
  {
    source: 'user',
    action: 'records_cleared',
    actionLabel: '清理探测记录',
    objectSnapshot: '全部探测记录',
    result: 'success',
    summary: '用户清理了全部站点的 48 条探测记录。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已清理全部探测记录。',
    content: { kind: 'cleanup', target: '探测记录', scope: '全部站点、全部保留范围', count: 48 },
  },
  {
    source: 'user',
    action: 'logs_cleared',
    actionLabel: '清理应用日志',
    objectSnapshot: '应用日志',
    result: 'success',
    summary: '用户清理了 7 个已轮转的应用日志文件，释放 18 MB 空间。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已清理应用日志文件。',
    content: {
      kind: 'cleanup',
      target: '应用日志',
      scope: '已轮转的日志文件',
      count: 7,
      space: '释放 18 MB',
    },
  },
  {
    source: 'user',
    action: 'diagnostics_exported',
    actionLabel: '导出诊断包',
    objectSnapshot: 'diagnostics-2026-11-04.zip',
    result: 'cancelled',
    summary: '用户取消了诊断包 diagnostics-2026-11-04.zip 的导出，未生成文件。',
    detailBehavior: { kind: 'drawer' },
    explanation: '诊断包导出已取消。',
    content: {
      kind: 'export',
      filename: 'diagnostics-2026-11-04.zip',
      fileSize: '未生成',
      outcome: '已取消',
    },
  },
  {
    source: 'system',
    action: 'scheduled_log_cleanup',
    actionLabel: '定时清理应用日志',
    objectSnapshot: '应用日志',
    result: 'success',
    summary: '系统定时清理了超过 14 天的 16 个应用日志文件，释放 42 MB 空间。',
    detailBehavior: { kind: 'drawer' },
    explanation: '系统按保留期限清理了应用日志。',
    content: {
      kind: 'cleanup',
      target: '应用日志',
      scope: '超过 14 天的轮转日志',
      count: 16,
      space: '释放 42 MB',
    },
  },
  {
    source: 'user',
    action: 'site_deleted',
    actionLabel: '删除站点',
    objectSnapshot: 'old-api.example',
    result: 'success',
    summary: '用户删除了站点 old-api.example 及其 3 个模型配置。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已删除站点 old-api.example。',
    content: {
      kind: 'deletion',
      summary: '删除前保存的名称快照和配置摘要。',
      fields: [
        { label: '协议', value: 'OpenAI 兼容协议' },
        { label: '模型数', value: '3' },
        { label: 'API 密钥', value: '已配置' },
      ],
    },
  },
  {
    source: 'user',
    action: 'model_status_changed',
    actionLabel: '调整模型探测状态',
    objectSnapshot: 'Claude Opus · Anthropic',
    result: 'success',
    summary: '用户停用了站点 Anthropic 中模型 Claude Opus 的自动探测。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已暂停 Claude Opus 的自动探测。',
    content: {
      kind: 'changes',
      changes: [{ field: '自动探测', before: '已启用', after: '已暂停' }],
    },
  },
  {
    source: 'user',
    action: 'notification_permission_requested',
    actionLabel: '请求通知授权',
    objectSnapshot: '桌面通知权限',
    result: 'failure',
    summary: '用户请求桌面通知授权失败，操作系统未授予通知权限。',
    detailBehavior: { kind: 'drawer' },
    explanation: '请求桌面通知授权未完成。',
    content: {
      kind: 'failure',
      activity: '请求通知授权',
      target: '桌面通知权限',
      reason: '操作系统未授予通知权限，请在系统设置中检查。',
    },
  },
  {
    source: 'system',
    action: 'automatic_probe',
    actionLabel: '自动任务受阻',
    objectSnapshot: '自动探测任务',
    result: 'blocked',
    summary: '系统未能启动站点 ai.input.im 的自动探测，应用正在退出。',
    detailBehavior: { kind: 'drawer' },
    explanation: '自动探测任务因应用状态不允许而受阻。',
    content: {
      kind: 'failure',
      activity: '启动自动探测任务',
      target: 'ai.input.im',
      reason: '应用正在退出，任务未执行。',
    },
  },
  {
    source: 'user',
    action: 'test_draft_saved',
    actionLabel: '保存测试草稿',
    objectSnapshot: '自定义探测草稿',
    result: 'success',
    summary: '用户保存了“自定义探测草稿”的测试请求内容。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已保存自定义探测草稿。',
    content: {
      kind: 'snapshot',
      fields: [
        { label: '请求方法', value: 'POST' },
        { label: '请求端点', value: '/v1/models' },
        { label: '请求头', value: 'Authorization: 已配置' },
      ],
    },
  },
  {
    source: 'user',
    action: 'records_cleared',
    actionLabel: '清理探测记录',
    objectSnapshot: '全部探测记录',
    result: 'failure',
    summary: '用户清理全部探测记录失败，数据库暂时不可用。',
    detailBehavior: { kind: 'drawer' },
    explanation: '清理探测记录失败。',
    content: {
      kind: 'failure',
      activity: '清理探测记录',
      target: '全部探测记录',
      reason: '数据库暂时不可用，请稍后重试。',
    },
  },
  {
    source: 'system',
    action: 'application_ready',
    actionLabel: '应用启动就绪',
    objectSnapshot: 'RelayPulse',
    result: 'success',
    summary: '系统完成 RelayPulse 启动，后台任务已就绪。',
    detailBehavior: { kind: 'drawer' },
    explanation: 'RelayPulse 已启动就绪。',
    content: { kind: 'snapshot', fields: [{ label: '后台任务', value: '已启动' }] },
  },
  {
    source: 'system',
    action: 'application_blocked',
    actionLabel: '应用启动受阻',
    objectSnapshot: 'RelayPulse',
    result: 'blocked',
    summary: '系统启动 RelayPulse 时通知初始化受阻，操作系统未授予通知权限。',
    detailBehavior: { kind: 'drawer' },
    explanation: '应用启动时通知初始化受阻。',
    content: {
      kind: 'failure',
      activity: '初始化桌面通知',
      target: 'RelayPulse 启动流程',
      reason: '操作系统暂未授予通知权限。',
    },
  },
  {
    source: 'user',
    action: 'model_deleted',
    actionLabel: '删除模型',
    objectSnapshot: 'GPT-4 Legacy · doudui.cc',
    result: 'success',
    summary: '用户删除了站点 doudui.cc 中的模型 GPT-4 Legacy。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已删除模型 GPT-4 Legacy。',
    content: {
      kind: 'deletion',
      summary: '模型从站点配置中移除。',
      fields: [
        { label: '模型', value: 'GPT-4 Legacy' },
        { label: '站点', value: 'doudui.cc' },
        { label: '探测状态', value: '已启用' },
      ],
    },
  },
  {
    source: 'user',
    action: 'site_status_changed',
    actionLabel: '调整站点状态',
    objectSnapshot: 'Anthropic',
    result: 'success',
    summary: '用户停用了站点 Anthropic 的自动探测。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已暂停 Anthropic 的自动探测。',
    content: {
      kind: 'changes',
      changes: [{ field: '自动探测', before: '已启用', after: '已暂停' }],
    },
  },
  {
    source: 'user',
    action: 'page_load_failed',
    actionLabel: '读取配置失败',
    objectSnapshot: '站点列表',
    result: 'failure',
    summary: '用户打开站点列表时读取站点配置失败，配置服务暂时不可用。',
    detailBehavior: { kind: 'drawer' },
    explanation: '读取站点列表失败。',
    content: {
      kind: 'failure',
      activity: '加载站点列表',
      target: '站点配置',
      reason: '配置服务暂时不可用，请重试。',
    },
  },
  {
    source: 'user',
    action: 'detail_read_failed',
    actionLabel: '读取操作详情失败',
    objectSnapshot: '通知模板',
    result: 'failure',
    summary: '用户读取“模型异常模板”详情失败，文件已移动或当前不可读取。',
    detailBehavior: { kind: 'drawer' },
    explanation: '读取通知模板详情失败。',
    content: {
      kind: 'failure',
      activity: '读取通知模板详情',
      target: '模型异常模板',
      reason: '文件已移动或当前不可读取。',
    },
  },
  {
    source: 'system',
    action: 'automatic_probe',
    actionLabel: '自动探测',
    objectSnapshot: 'old-api.example',
    result: 'failure',
    summary: '系统对站点 old-api.example 的自动探测失败，站点凭据缺失，未生成探测记录。',
    detailBehavior: { kind: 'probe_records', recordIds: [], availability: 'none' },
    explanation: '自动探测未能开始请求，因此没有生成探测记录。',
    content: { kind: 'probe_failure', reason: '站点凭据缺失，探测请求未发出。' },
  },
  {
    source: 'system',
    action: 'automatic_probe',
    actionLabel: '自动探测',
    objectSnapshot: 'archive-api.example',
    result: 'success',
    summary: '系统完成站点 archive-api.example 的自动探测，关联探测记录现已过期或清理。',
    detailBehavior: {
      kind: 'probe_records',
      recordIds: ['probe-expired-001'],
      availability: 'expired',
    },
    explanation: '此事件关联的探测记录已清理。',
    content: { kind: 'probe_failure', reason: '关联的探测记录已过期或已被清理。' },
  },
  {
    source: 'user',
    action: 'diagnostics_exported',
    actionLabel: '导出诊断包',
    objectSnapshot: 'diagnostics-session.zip',
    result: 'success',
    summary: '用户导出了诊断包 diagnostics-session.zip，并保存到所选位置。',
    detailBehavior: { kind: 'drawer' },
    explanation: '已完成诊断包导出，文件已保存。',
    content: {
      kind: 'export',
      filename: 'diagnostics-session.zip',
      fileSize: '2.4 MB',
      outcome: '导出成功，已保存到用户选择的位置',
    },
  },
];

const fixtureTime = Date.now();
const logs: OperationLogListItem[] = fixtures.map((fixture, index) => ({
  id: `op-${String(index + 1).padStart(3, '0')}`,
  occurredAt: fixture.occurredAt ?? new Date(fixtureTime - index * 53 * 60_000).toISOString(),
  source: fixture.source,
  actorSnapshot: fixture.actorSnapshot ?? (fixture.source === 'system' ? '系统任务' : 'Scott'),
  action: fixture.action,
  actionGroup: operationActionGroupByAction[fixture.action],
  actionLabel: fixture.actionLabel,
  objectSnapshot: fixture.objectSnapshot,
  result: fixture.result,
  summary: fixture.summary,
  detailBehavior: fixture.detailBehavior,
}));
const details = new Map<string, OperationLogDetail>(
  fixtures.map((fixture, index) => [
    `op-${String(index + 1).padStart(3, '0')}`,
    { item: logs[index], explanation: fixture.explanation, content: fixture.content },
  ]),
);

function scenario() {
  return new URLSearchParams(window.location.search).get('operationScenario');
}

function validation(message: string, field: string): never {
  throw new RelayPulseBackendError({
    code: 'operation_logs_validation',
    category: 'validation',
    message,
    field,
    retryable: false,
  });
}

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    return validation('操作记录请求参数无效', 'query');
  return value as Record<string, unknown>;
}

function validDate(value: unknown): value is string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const time = Date.parse(`${value}T00:00:00Z`);
  return (
    Number.isFinite(time) &&
    !value.startsWith('0000') &&
    new Date(time).toISOString().slice(0, 10) === value
  );
}

function validQuery(value: unknown): OperationLogQuery {
  const query = object(object(value).query);
  const { startDate, endDate, source, result, actions, page, pageSize } = query;
  if (typeof page !== 'number' || !Number.isSafeInteger(page) || page < 1)
    validation('页码必须是从 1 开始的整数', 'query.page');
  if (typeof pageSize !== 'number' || ![10, 20, 50, 100].includes(pageSize))
    validation('每页条数必须为 10、20、50 或 100', 'query.pageSize');
  const allowedSources = ['system', 'user'];
  const allowedResults = ['success', 'failure', 'cancelled', 'blocked'];
  const allowedActions = [
    'probe',
    'create',
    'update',
    'delete',
    'sync',
    'cleanup',
    'notification',
    'authorization',
    'export',
    'startup',
    'read',
    'status_change',
  ];
  if (!Array.isArray(source) || source.some((value) => !allowedSources.includes(value)))
    validation('操作来源筛选无效', 'query.source');
  if (!Array.isArray(result) || result.some((value) => !allowedResults.includes(value)))
    validation('执行结果筛选无效', 'query.result');
  if (!Array.isArray(actions) || actions.some((value) => !allowedActions.includes(value)))
    validation('操作动作筛选无效', 'query.actions');
  if (startDate !== null && !validDate(startDate))
    validation('开始日期必须为有效的 YYYY-MM-DD 日期或 null', 'query.startDate');
  if (endDate !== null && !validDate(endDate))
    validation('结束日期必须为有效的 YYYY-MM-DD 日期或 null', 'query.endDate');
  if (startDate && endDate && startDate > endDate)
    validation('结束日期不能早于开始日期', 'query.endDate');
  return { startDate, endDate, source, result, actions, page, pageSize };
}

/** 日期筛选包含设备本地日期的开始与结束全天，与页面的时间展示使用相同口径。 */
function matchesDate(value: string, startDate: string | null, endDate: string | null) {
  const time = new Date(value);
  const date = `${time.getFullYear()}-${String(time.getMonth() + 1).padStart(2, '0')}-${String(time.getDate()).padStart(2, '0')}`;
  if (startDate && date < startDate) return false;
  if (endDate && date > endDate) return false;
  return true;
}

const failedOnce = new Set<string>();

function register(
  command: 'get_operation_logs' | 'get_operation_log_detail',
  handler: (args: unknown, activeScenario: string | null) => unknown,
) {
  registerMockHandler(command, async (args) => {
    const input = args === undefined ? undefined : structuredClone(args);
    const activeScenario = scenario();
    const configured = new URLSearchParams(window.location.search).get('operationDelay');
    let delay = import.meta.env.MODE === 'test' ? 0 : 450;
    if (activeScenario === 'slow') delay = 2200;
    if (configured !== null && Number.isFinite(Number(configured)))
      delay = Math.max(0, Math.min(10_000, Number(configured)));
    if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
    if (activeScenario === 'retry-once' && !failedOnce.has(command)) {
      failedOnce.add(command);
      throw new RelayPulseBackendError({
        code: 'operation_logs_unavailable',
        category: 'unavailable',
        message:
          command === 'get_operation_logs'
            ? '暂时无法读取操作记录，请重试'
            : '暂时无法读取这条操作详情，请重试',
        retryable: true,
      });
    }
    return handler(input, activeScenario);
  });
}

register('get_operation_logs', (args, activeScenario): OperationLogPage => {
  if (activeScenario === 'error' || activeScenario === 'list-error')
    throw new RelayPulseBackendError({
      code: 'operation_logs_unavailable',
      category: 'unavailable',
      message: '暂时无法读取操作记录，请重试',
      retryable: true,
    });
  const query = validQuery(args);
  const filtered = (activeScenario === 'empty' ? [] : logs).filter(
    (item) =>
      (!query.source.length || query.source.includes(item.source)) &&
      (!query.result.length || query.result.includes(item.result)) &&
      (!query.actions.length || query.actions.includes(item.actionGroup)) &&
      matchesDate(item.occurredAt, query.startDate, query.endDate),
  );
  filtered.sort(
    (a, b) => Date.parse(b.occurredAt) - Date.parse(a.occurredAt) || a.id.localeCompare(b.id),
  );
  const pageSize = Number(query.pageSize);
  const page = Math.min(Number(query.page), Math.max(1, Math.ceil(filtered.length / pageSize)));
  return {
    items: filtered
      .slice((page - 1) * pageSize, page * pageSize)
      .map((item) => structuredClone(item)),
    total: filtered.length,
    page,
    pageSize,
  };
});

register('get_operation_log_detail', (args, activeScenario): OperationLogDetail => {
  if (activeScenario === 'error' || activeScenario === 'detail-error')
    throw new RelayPulseBackendError({
      code: 'operation_log_detail_unavailable',
      category: 'unavailable',
      message: '暂时无法读取这条操作详情，请重试',
      retryable: true,
    });
  const logId = object(args).logId;
  if (typeof logId !== 'string' || !details.has(logId) || activeScenario === 'detail-missing')
    throw new RelayPulseBackendError({
      code: 'operation_log_not_found',
      category: 'not_found',
      message: '这条操作记录已不存在或已过期',
      retryable: false,
    });
  return structuredClone(details.get(logId)!);
});

export { logs as operationLogFixtures };

/** IQ 浏览器操作夹具，独立于通知开关；不写入真实数据库或诊断日志。 */
export function appendMockIqOperation(
  action: OperationAction,
  actionLabel: string,
  objectSnapshot: string,
) {
  const log: OperationLogListItem = {
    id: crypto.randomUUID(),
    occurredAt: new Date().toISOString(),
    source: 'user',
    actorSnapshot: '当前用户',
    action,
    actionGroup: operationActionGroupByAction[action],
    actionLabel,
    objectSnapshot,
    result: 'success',
    summary: `${actionLabel}：${objectSnapshot}`,
    detailBehavior: { kind: 'drawer' },
  };
  logs.unshift(log);
  details.set(log.id, {
    item: log,
    explanation: log.summary,
    content: {
      kind: 'snapshot',
      fields: [
        { label: '操作对象', value: objectSnapshot },
        { label: '数据来源', value: '浏览器模拟会话' },
      ],
    },
  });
}
