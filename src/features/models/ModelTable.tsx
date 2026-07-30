import { SettingOutlined } from '@ant-design/icons';
import { Button, Switch, Table, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { Model, Site } from '../../domain/types';
import { navigateToView } from '../../app/view';
import styles from '../../styles/App.module.css';

export function ModelTable({
  site,
  onEnabledChange,
}: {
  site: Site;
  onEnabledChange(modelId: string, enabled: boolean): void;
}) {
  const columns: ColumnsType<Model> = [
    {
      title: '启用',
      dataIndex: 'enabled',
      width: 72,
      align: 'center',
      render: (_, model) => (
        <Switch
          size="small"
          checked={model.enabled}
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
        <span className={styles.modelName} title={name}>
          {name}
        </span>
      ),
    },
    {
      title: '推理设置',
      dataIndex: 'inferenceSummary',
      width: 190,
      ellipsis: true,
      className: styles.secondaryCell,
    },
    { title: '首 Token', dataIndex: 'firstToken', width: 125 },
    { title: '总耗时', dataIndex: 'totalTime', width: 110 },
    {
      title: '操作',
      key: 'action',
      width: 64,
      align: 'center',
      render: (_, model) => (
        <Tooltip title="模型探针设置">
          <Button
            type="text"
            shape="circle"
            icon={<SettingOutlined />}
            aria-label={`设置模型 ${model.name}`}
            className={styles.tableAction}
            onClick={() => {
              const url = new URL(window.location.href);
              url.searchParams.set('model', model.id);
              url.searchParams.delete('scope');
              window.history.replaceState({}, '', url);
              navigateToView('model-settings');
            }}
          />
        </Tooltip>
      ),
    },
  ];

  return (
    <Table<Model>
      className={styles.modelTable}
      rowKey="id"
      columns={columns}
      dataSource={site.models}
      pagination={false}
      size="small"
      tableLayout="fixed"
    />
  );
}
