import { DeleteOutlined, PlayCircleOutlined, SettingOutlined } from '@ant-design/icons';
import { Button, Popconfirm, Switch, Table, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { Model, Site } from '../../domain/types';
import { navigateToView } from '../../app/view';
import { FirstTokenColumnTitle } from '../../components/FirstTokenColumnTitle';

export function ModelTable({
  site,
  loadingModelId,
  deletingModelId,
  probingModelId,
  siteProbeRunning,
  onEnabledChange,
  onProbe,
  onDelete,
}: {
  site: Site;
  loadingModelId?: string | null;
  deletingModelId?: string | null;
  probingModelId?: string | null;
  siteProbeRunning?: boolean;
  onEnabledChange(modelId: string, enabled: boolean): void;
  onProbe(modelId: string): Promise<void>;
  onDelete(modelId: string): Promise<void>;
}) {
  const busyModelId = loadingModelId ?? deletingModelId ?? probingModelId;
  const tableBusy = Boolean(siteProbeRunning || busyModelId);
  const columns: ColumnsType<Model> = [
    {
      title: '启用',
      dataIndex: 'enabled',
      width: 72,
      align: 'center',
      fixed: 'left',
      render: (_, model) => (
        <Switch
          size="small"
          checked={model.enabled}
          loading={loadingModelId === model.id}
          disabled={tableBusy}
          aria-label={`${model.enabled ? '停用' : '启用'}模型 ${model.name}`}
          onChange={(enabled) => onEnabledChange(model.id, enabled)}
        />
      ),
    },
    {
      title: '模型',
      dataIndex: 'name',
      width: 230,
      ellipsis: true,
      render: (name: string) => (
        <span className="block truncate font-semibold" title={name}>
          {name}
        </span>
      ),
    },
    {
      title: '推理设置',
      dataIndex: 'inferenceSummary',
      width: 190,
      ellipsis: true,
      className: 'text-rp-secondary',
    },
    { title: <FirstTokenColumnTitle />, dataIndex: 'firstToken', width: 125 },
    { title: '总耗时', dataIndex: 'totalTime', width: 110 },
    {
      title: '操作',
      key: 'action',
      width: 140,
      align: 'center',
      fixed: 'right',
      render: (_, model) => (
        <div className="flex items-center justify-center gap-1">
          <Tooltip title="立即探测模型">
            <Button
              type="text"
              shape="circle"
              icon={<PlayCircleOutlined />}
              loading={probingModelId === model.id}
              aria-label={`立即探测模型 ${model.name}`}
              className="size-8! text-rp-primary!"
              disabled={Boolean(siteProbeRunning || (busyModelId && probingModelId !== model.id))}
              onClick={() => void onProbe(model.id)}
            />
          </Tooltip>
          <Tooltip title="模型探针设置">
            <Button
              type="text"
              shape="circle"
              icon={<SettingOutlined />}
              aria-label={`设置模型 ${model.name}`}
              className="size-8! text-rp-primary!"
              disabled={tableBusy}
              onClick={() => {
                const url = new URL(window.location.href);
                url.searchParams.set('model', model.id);
                url.searchParams.delete('scope');
                window.history.replaceState({}, '', url);
                navigateToView('model-settings');
              }}
            />
          </Tooltip>
          <Popconfirm
            title={`删除模型 ${model.name}？`}
            description="相关探测记录也会一并删除。"
            okText="删除"
            cancelText="取消"
            okButtonProps={{ danger: true }}
            disabled={Boolean(siteProbeRunning || (busyModelId && deletingModelId !== model.id))}
            onConfirm={() => void onDelete(model.id)}
          >
            <Button
              type="text"
              shape="circle"
              danger
              icon={<DeleteOutlined />}
              loading={deletingModelId === model.id}
              disabled={Boolean(siteProbeRunning || (busyModelId && deletingModelId !== model.id))}
              aria-label={`删除模型 ${model.name}`}
              className="size-8!"
            />
          </Popconfirm>
        </div>
      ),
    },
  ];

  return (
    <Table<Model>
      rowKey="id"
      columns={columns}
      dataSource={site.models}
      pagination={false}
      size="small"
      tableLayout="fixed"
      className="rp-table-scrollbar"
      scroll={{ x: 900 }}
    />
  );
}
