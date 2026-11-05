import type { OperationLogDetailContent } from '@/types/domain/operation-logs';

import { describe, expect, it } from 'vitest';

import { presentOperationContent } from '@/views/run/operation-history/operation-log-presentation';

describe('操作记录字段展示', () => {
  it('使用同次事件中的历史模型名称解释字段路径和配置状态', () => {
    const content: OperationLogDetailContent = {
      kind: 'changes',
      changes: [
        { field: 'models[1].customRequest', before: '未配置', after: 'null' },
        { field: 'models[1].policy', before: '未配置', after: 'null' },
        { field: 'models[1].enabled', before: 'false', after: 'true' },
        { field: 'models[1].id', before: '未配置', after: 'historical-model-id' },
        { field: 'models[1].name', before: '未配置', after: 'gpt-6.1-sol' },
      ],
    };

    const presented = presentOperationContent(content, 'models_synchronized');

    expect(presented).toMatchObject({
      kind: 'changes',
      changes: [
        {
          field: '模型 2「gpt-6.1-sol」 · 自定义探针请求',
          before: '未配置',
          after: '未启用',
        },
        { after: '继承站点默认' },
        { before: '已停用', after: '已启用' },
        { field: expect.stringContaining('模型标识'), after: 'historical-model-id' },
        { after: 'gpt-6.1-sol' },
      ],
    });
    expect(content.changes[0]).toEqual({
      field: 'models[1].customRequest',
      before: '未配置',
      after: 'null',
    });
  });

  it('事件未携带模型名称时使用原始序号，并保留缺失值文案', () => {
    expect(
      presentOperationContent(
        {
          kind: 'changes',
          changes: [{ field: 'models[1].enabled', before: '未配置', after: 'false' }],
        },
        'model_status_changed',
      ),
    ).toMatchObject({
      changes: [{ field: '模型 2 · 探测状态', before: '未配置', after: '已停用' }],
    });
  });

  it('数组位置更换对象时同时展示前后名称，避免把旧值归给新模型', () => {
    expect(
      presentOperationContent(
        {
          kind: 'changes',
          changes: [
            { field: 'models[0].name', before: '原模型', after: '后移模型' },
            { field: 'models[0].enabled', before: 'true', after: 'false' },
          ],
        },
        'models_synchronized',
      ),
    ).toMatchObject({
      changes: [
        { field: '模型 1「原模型 → 后移模型」 · 模型名称' },
        { field: '模型 1「原模型 → 后移模型」 · 探测状态' },
      ],
    });
  });

  it('按前后各自的请求模式解释空独立策略', () => {
    expect(
      presentOperationContent(
        {
          kind: 'changes',
          changes: [
            { field: 'models[0].policy', before: 'null', after: 'null' },
            { field: 'models[0].customRequest.endpoint', before: '未配置', after: '/probe' },
          ],
        },
        'test_draft_saved',
      ),
    ).toMatchObject({
      changes: [
        { before: '继承站点默认', after: '未配置（使用自定义探针请求）' },
        { after: '/probe' },
      ],
    });
  });

  it('使用事件所属界面的设置标签，并保留通知模板原文', () => {
    expect(
      presentOperationContent(
        { kind: 'changes', changes: [{ field: 'theme', before: 'system', after: 'dark' }] },
        'general_settings_updated',
      ),
    ).toMatchObject({ changes: [{ field: '主题', before: '跟随系统', after: '深色' }] });
    expect(
      presentOperationContent(
        { kind: 'changes', changes: [{ field: 'titleTemplate', before: 'null', after: 'true' }] },
        'notification_template_updated',
      ),
    ).toMatchObject({ changes: [{ field: '标题', before: 'null', after: 'true' }] });
    expect(
      presentOperationContent(
        { kind: 'snapshot', fields: [{ label: '提交内容.draft.endpoint', value: '/probe' }] },
        'manual_probe',
      ),
    ).toMatchObject({
      fields: [{ label: '提交内容 · 探针设置草稿 · 自定义探测端点', value: '/probe' }],
    });
  });

  it.each(['snapshot', 'deletion'] as const)('%s 共用模型字段和值的展示规则', (kind) => {
    const fields = [
      { label: 'models[1].name', value: 'gpt-6.1-sol' },
      { label: 'models[1].customRequest', value: 'null' },
      { label: 'models[1].enabled', value: 'true' },
    ];
    const content: OperationLogDetailContent =
      kind === 'deletion' ? { kind, summary: '站点及其模型已删除', fields } : { kind, fields };

    expect(presentOperationContent(content, 'site_deleted')).toMatchObject({
      kind,
      fields: [
        { value: 'gpt-6.1-sol' },
        { label: '模型 2「gpt-6.1-sol」 · 自定义探针请求', value: '未启用' },
        { value: '已启用' },
      ],
    });
    if (kind === 'deletion') {
      expect(presentOperationContent(content, 'site_deleted')).toMatchObject({
        summary: '站点及其模型已删除',
      });
    }
  });

  it('保持请求参数的用户原文，不把参数值误当作开关或空配置', () => {
    expect(
      presentOperationContent(
        {
          kind: 'changes',
          changes: [
            { field: 'policy.parameters[0].key', before: 'old', after: 'stream' },
            { field: 'policy.parameters[0].value', before: 'true', after: 'null' },
            { field: 'policy.parameters[1].key', before: 'old', after: 'metadata' },
            { field: 'policy.parameters[1].value', before: '[]', after: '{}' },
          ],
        },
        'probe_config_saved',
      ),
    ).toMatchObject({
      changes: [
        { before: 'old', after: 'stream' },
        { before: 'true', after: 'null' },
        { before: 'old', after: 'metadata' },
        { before: '[]', after: '{}' },
      ],
    });
  });

  it('敏感快照只展示后端约定的配置状态', () => {
    expect(
      presentOperationContent(
        {
          kind: 'snapshot',
          fields: [
            { label: '凭据', value: '未配置', sensitive: true },
            { label: 'credentialConfigured', value: '已配置', sensitive: true },
            { label: 'connection.headers', value: '已更换', sensitive: true },
            { label: 'secret', value: 'unexpected-private-value', sensitive: true },
          ],
        },
        'site_created',
      ),
    ).toMatchObject({
      fields: [
        { value: '未配置', sensitive: true },
        { value: '已配置', sensitive: true },
        { value: '已更换', sensitive: true },
        { value: '已隐藏', sensitive: true },
      ],
    });
  });

  it('敏感变更分别处理修改前后的状态，不制造已配置或已更换的事实', () => {
    expect(
      presentOperationContent(
        {
          kind: 'changes',
          changes: [
            { field: '凭据', before: '未配置', after: '已配置', sensitive: true },
            {
              field: 'connection.headers',
              before: '已配置',
              after: '已更换',
              sensitive: true,
            },
            { field: 'secret', before: 'old-secret', after: 'new-secret', sensitive: true },
          ],
        },
        'site_updated',
      ),
    ).toMatchObject({
      changes: [
        { before: '未配置', after: '已配置', sensitive: true },
        { before: '已配置', after: '已更换', sensitive: true },
        { before: '已隐藏', after: '已隐藏', sensitive: true },
      ],
    });
  });

  it('未知字段、未知值及其他详情内容保持原样', () => {
    const contents: OperationLogDetailContent[] = [
      {
        kind: 'snapshot',
        fields: [{ label: 'future.field[0]', value: 'true' }],
      },
      {
        kind: 'changes',
        changes: [{ field: 'future.field[0]', before: 'null', after: '[]' }],
      },
      {
        kind: 'deletion',
        summary: '历史对象已删除',
        fields: [{ label: 'future.field[0]', value: 'false' }],
      },
      { kind: 'probe_failure', reason: '远端返回 null' },
    ];
    for (const content of contents) {
      expect(presentOperationContent(content, 'site_updated')).toEqual(content);
    }
  });
});
