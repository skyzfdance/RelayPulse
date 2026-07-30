import {
  DeleteOutlined,
  FileSearchOutlined,
  MoreOutlined,
  ReloadOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import {
  Button,
  Drawer,
  Dropdown,
  Input,
  Modal,
  Segmented,
  Select,
  Table,
  Tag,
  Tooltip,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { StatusDot } from '../../components/StatusDot';
import type { ProbeRecord, ProbeRecordStatus } from '../../domain/types';
import styles from '../../styles/App.module.css';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

type RecordsMode = 'records' | 'usage';
type PeriodFilter = '24h' | '7d' | '30d';
type StatusFilter = 'all' | ProbeRecordStatus;

const periodMilliseconds: Record<PeriodFilter, number> = {
  '24h': 24 * 60 * 60 * 1000,
  '7d': 7 * 24 * 60 * 60 * 1000,
  '30d': 30 * 24 * 60 * 60 * 1000,
};

const statusMeta: Record<ProbeRecordStatus, { label: string; color?: string }> = {
  healthy: { label: '正常', color: 'green' },
  warning: { label: '偏慢', color: 'gold' },
  error: { label: '失败', color: 'red' },
};

function formatTimestamp(value: string) {
  return `${value.slice(5, 10)} ${value.slice(11, 19)}`;
}

function formatTokens(value: number | null) {
  return value === null ? '—' : new Intl.NumberFormat('zh-CN').format(value);
}

function RecordStatus({ status }: { status: ProbeRecordStatus }) {
  return (
    <span className={styles.recordStatus}>
      <StatusDot status={status} />
      {statusMeta[status].label}
    </span>
  );
}

function TokenValue({ record }: { record: ProbeRecord }) {
  if (record.tokenUsage.source === 'unavailable') {
    return (
      <Tooltip title="供应商未返回 usage">
        <span className={styles.unavailableValue}>—</span>
      </Tooltip>
    );
  }
  return <span>{formatTokens(record.tokenUsage.totalTokens)}</span>;
}

interface SiteUsageSummary {
  siteId: string;
  siteName: string;
  probes: number;
  reportedProbes: number;
  unavailableProbes: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  averageTokens: number | null;
}

function buildUsageSummary(records: ProbeRecord[]): SiteUsageSummary[] {
  const bySite = new Map<string, SiteUsageSummary>();
  for (const record of records) {
    const summary = bySite.get(record.siteId) ?? {
      siteId: record.siteId,
      siteName: record.siteName,
      probes: 0,
      reportedProbes: 0,
      unavailableProbes: 0,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      averageTokens: null,
    };
    summary.probes += 1;
    if (record.tokenUsage.source === 'reported') {
      summary.reportedProbes += 1;
      summary.inputTokens += record.tokenUsage.inputTokens ?? 0;
      summary.outputTokens += record.tokenUsage.outputTokens ?? 0;
      summary.totalTokens += record.tokenUsage.totalTokens ?? 0;
    } else {
      summary.unavailableProbes += 1;
    }
    bySite.set(record.siteId, summary);
  }

  return [...bySite.values()]
    .map((summary) => ({
      ...summary,
      averageTokens:
        summary.reportedProbes > 0
          ? Math.round(summary.totalTokens / summary.reportedProbes)
          : null,
    }))
    .sort((left, right) => right.totalTokens - left.totalTokens);
}

function RecordDetailDrawer({ record, onClose }: { record: ProbeRecord; onClose(): void }) {
  const usageReported = record.tokenUsage.source === 'reported';
  return (
    <Drawer
      open
      size={520}
      closable={{ placement: 'end' }}
      title={
        <div>
          <strong>探测详情</strong>
          <div className={styles.drawerSubtitle}>
            {record.modelName} · {record.siteName}
          </div>
        </div>
      }
      getContainer={overlayContainer}
      rootClassName={styles.scopedOverlay}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: true }}
      onClose={onClose}
    >
      <div className={styles.recordDetailLead}>
        <div>
          <RecordStatus status={record.status} />
          <span>{formatTimestamp(record.startedAt)}</span>
        </div>
        <strong>{record.result}</strong>
      </div>

      <section className={styles.drawerSection}>
        <h3>性能与用量</h3>
        <dl className={styles.recordDetailGrid}>
          <div>
            <dt>首 Token</dt>
            <dd>{record.firstTokenLabel}</dd>
          </div>
          <div>
            <dt>总耗时</dt>
            <dd>{record.totalTimeLabel}</dd>
          </div>
          <div>
            <dt>输入 Token</dt>
            <dd>{formatTokens(record.tokenUsage.inputTokens)}</dd>
          </div>
          <div>
            <dt>输出 Token</dt>
            <dd>{formatTokens(record.tokenUsage.outputTokens)}</dd>
          </div>
          <div>
            <dt>总 Token</dt>
            <dd>{formatTokens(record.tokenUsage.totalTokens)}</dd>
          </div>
          <div>
            <dt>用量来源</dt>
            <dd>
              <Tag color={usageReported ? 'blue' : undefined}>
                {usageReported ? '供应商上报' : '未上报'}
              </Tag>
            </dd>
          </div>
        </dl>
      </section>

      <section className={styles.drawerSection}>
        <h3>请求信息</h3>
        <dl className={styles.recordRequestList}>
          <div>
            <dt>请求端点</dt>
            <dd className={styles.mono}>{record.endpoint}</dd>
          </div>
          <div>
            <dt>探针策略</dt>
            <dd>{record.probeMode}</dd>
          </div>
          <div>
            <dt>HTTP 状态</dt>
            <dd>{record.httpStatus ?? '未收到响应'}</dd>
          </div>
          <div>
            <dt>请求 ID</dt>
            <dd className={styles.mono}>{record.requestId ?? '—'}</dd>
          </div>
        </dl>
      </section>

      {record.errorDetail ? (
        <section className={styles.drawerSection}>
          <h3>错误信息</h3>
          <div className={styles.recordErrorBox}>{record.errorDetail}</div>
        </section>
      ) : null}
    </Drawer>
  );
}

export function RecordsPage() {
  const { snapshot, refreshProbeRecords, clearProbeRecords } = useRelayPulse();
  const [mode, setMode] = useState<RecordsMode>('records');
  const [period, setPeriod] = useState<PeriodFilter>('24h');
  const [siteFilter, setSiteFilter] = useState('all');
  const [modelFilter, setModelFilter] = useState('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [query, setQuery] = useState('');
  const [selectedRecord, setSelectedRecord] = useState<ProbeRecord | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [clearing, setClearing] = useState(false);

  const allRecords = useMemo(() => snapshot?.probeRecords ?? [], [snapshot?.probeRecords]);
  const periodRecords = useMemo(() => {
    const latestTimestamp = Math.max(
      ...allRecords.map((record) => Date.parse(record.startedAt)),
      0,
    );
    const cutoff = latestTimestamp - periodMilliseconds[period];
    return allRecords.filter((record) => Date.parse(record.startedAt) >= cutoff);
  }, [allRecords, period]);

  const filteredRecords = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase('zh-CN');
    return periodRecords.filter((record) => {
      if (siteFilter !== 'all' && record.siteId !== siteFilter) return false;
      if (modelFilter !== 'all' && record.modelId !== modelFilter) return false;
      if (statusFilter !== 'all' && record.status !== statusFilter) return false;
      if (!normalizedQuery) return true;
      return [record.siteName, record.modelName, record.result].some((value) =>
        value.toLocaleLowerCase('zh-CN').includes(normalizedQuery),
      );
    });
  }, [modelFilter, periodRecords, query, siteFilter, statusFilter]);

  const usageRecords = useMemo(
    () => periodRecords.filter((record) => siteFilter === 'all' || record.siteId === siteFilter),
    [periodRecords, siteFilter],
  );
  const usageSummary = useMemo(() => buildUsageSummary(usageRecords), [usageRecords]);
  const reportedCount = usageRecords.filter(
    (record) => record.tokenUsage.source === 'reported',
  ).length;
  const totalTokens = usageRecords.reduce(
    (total, record) => total + (record.tokenUsage.totalTokens ?? 0),
    0,
  );
  const unavailableCount = usageRecords.length - reportedCount;

  const siteOptions = useMemo(
    () => [
      { value: 'all', label: '全部站点' },
      ...Array.from(
        new Map(allRecords.map((record) => [record.siteId, record.siteName])).entries(),
      ).map(([value, label]) => ({ value, label })),
    ],
    [allRecords],
  );
  const modelOptions = useMemo(
    () => [
      { value: 'all', label: '全部模型' },
      ...Array.from(
        new Map(
          periodRecords
            .filter((record) => siteFilter === 'all' || record.siteId === siteFilter)
            .map((record) => [record.modelId, record.modelName]),
        ).entries(),
      ).map(([value, label]) => ({ value, label })),
    ],
    [periodRecords, siteFilter],
  );

  const recordColumns: ColumnsType<ProbeRecord> = [
    {
      title: '状态',
      dataIndex: 'status',
      width: 86,
      render: (status: ProbeRecordStatus) => <RecordStatus status={status} />,
    },
    {
      title: '探测时间',
      dataIndex: 'startedAt',
      width: 145,
      sorter: (left, right) => Date.parse(left.startedAt) - Date.parse(right.startedAt),
      defaultSortOrder: 'descend',
      render: (value: string) => (
        <span className={styles.recordTime}>{formatTimestamp(value)}</span>
      ),
    },
    {
      title: '站点',
      dataIndex: 'siteName',
      width: 150,
      ellipsis: { showTitle: false },
      render: (value: string) => <span title={value}>{value}</span>,
    },
    {
      title: '模型',
      dataIndex: 'modelName',
      width: 180,
      ellipsis: { showTitle: false },
      render: (value: string) => (
        <span className={styles.modelName} title={value}>
          {value}
        </span>
      ),
    },
    {
      title: '首 Token',
      dataIndex: 'firstTokenLabel',
      width: 112,
      sorter: (left, right) => (left.firstTokenMs ?? Infinity) - (right.firstTokenMs ?? Infinity),
    },
    {
      title: '总耗时',
      dataIndex: 'totalTimeLabel',
      width: 105,
      sorter: (left, right) => left.totalTimeMs - right.totalTimeMs,
    },
    {
      title: 'Token',
      key: 'tokens',
      width: 92,
      align: 'right',
      sorter: (left, right) =>
        (left.tokenUsage.totalTokens ?? -1) - (right.tokenUsage.totalTokens ?? -1),
      render: (_, record) => <TokenValue record={record} />,
    },
    {
      title: '结果',
      dataIndex: 'result',
      width: 180,
      ellipsis: { showTitle: false },
      render: (value: string, record) => (
        <span
          className={record.status === 'error' ? styles.recordResultError : undefined}
          title={value}
        >
          {value}
        </span>
      ),
    },
    {
      title: '详情',
      key: 'action',
      width: 60,
      align: 'center',
      render: (_, record) => (
        <Tooltip title="查看探测详情">
          <Button
            type="text"
            shape="circle"
            icon={<FileSearchOutlined />}
            aria-label={`查看 ${record.modelName} 探测详情`}
            className={styles.tableAction}
            onClick={() => setSelectedRecord(record)}
          />
        </Tooltip>
      ),
    },
  ];

  const usageColumns: ColumnsType<SiteUsageSummary> = [
    {
      title: '站点',
      dataIndex: 'siteName',
      width: 240,
      render: (value: string) => <strong>{value}</strong>,
    },
    { title: '探测次数', dataIndex: 'probes', width: 120, align: 'right' },
    {
      title: '已上报',
      key: 'reported',
      width: 130,
      align: 'right',
      render: (_, summary) => `${summary.reportedProbes} / ${summary.probes}`,
    },
    {
      title: '输入 Token',
      dataIndex: 'inputTokens',
      width: 140,
      align: 'right',
      render: (value: number, summary) => (summary.reportedProbes > 0 ? formatTokens(value) : '—'),
      sorter: (left, right) => left.inputTokens - right.inputTokens,
    },
    {
      title: '输出 Token',
      dataIndex: 'outputTokens',
      width: 140,
      align: 'right',
      render: (value: number, summary) => (summary.reportedProbes > 0 ? formatTokens(value) : '—'),
      sorter: (left, right) => left.outputTokens - right.outputTokens,
    },
    {
      title: '总 Token',
      dataIndex: 'totalTokens',
      width: 140,
      align: 'right',
      defaultSortOrder: 'descend',
      render: (value: number, summary) =>
        summary.reportedProbes > 0 ? <strong>{formatTokens(value)}</strong> : '—',
      sorter: (left, right) => left.totalTokens - right.totalTokens,
    },
    {
      title: '平均每次',
      dataIndex: 'averageTokens',
      width: 140,
      align: 'right',
      render: (value: number | null) => formatTokens(value),
    },
  ];

  if (!snapshot) return null;

  async function refresh() {
    setRefreshing(true);
    try {
      await refreshProbeRecords();
    } finally {
      setRefreshing(false);
    }
  }

  async function clearRecords() {
    setClearing(true);
    try {
      await clearProbeRecords();
      setSelectedRecord(null);
      setClearConfirmOpen(false);
    } finally {
      setClearing(false);
    }
  }

  return (
    <>
      <header className={styles.pageHeader}>
        <div>
          <h1>探测记录</h1>
          <p>查看模型可用性、响应耗时和供应商上报的 Token 用量</p>
        </div>
        <div className={styles.recordsHeaderActions}>
          <Tooltip title="刷新记录">
            <Button
              shape="circle"
              icon={<ReloadOutlined />}
              loading={refreshing}
              aria-label="刷新探测记录"
              onClick={() => void refresh()}
            />
          </Tooltip>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                {
                  key: 'clear',
                  label: '清理全部记录',
                  icon: <DeleteOutlined />,
                  danger: true,
                  disabled: allRecords.length === 0,
                },
              ],
              onClick: ({ key }) => {
                if (key === 'clear') setClearConfirmOpen(true);
              },
            }}
          >
            <Button shape="circle" icon={<MoreOutlined />} aria-label="更多记录操作" />
          </Dropdown>
        </div>
      </header>

      <main className={styles.recordsContent}>
        <div className={styles.recordsModeBar}>
          <Segmented
            value={mode}
            options={[
              { value: 'records', label: '探测记录' },
              { value: 'usage', label: 'Token 统计' },
            ]}
            onChange={(value) => setMode(value as RecordsMode)}
          />
          <span>记录保留 {snapshot.generalSettings.historyRetentionDays} 天</span>
        </div>

        <div className={styles.recordsFilters}>
          <Select<PeriodFilter>
            value={period}
            aria-label="统计周期"
            options={[
              { value: '24h', label: '最近 24 小时' },
              { value: '7d', label: '最近 7 天' },
              { value: '30d', label: '最近 30 天' },
            ]}
            onChange={setPeriod}
          />
          <Select
            value={siteFilter}
            aria-label="站点筛选"
            options={siteOptions}
            onChange={(value) => {
              setSiteFilter(value);
              setModelFilter('all');
            }}
          />
          {mode === 'records' ? (
            <>
              <Select
                value={modelFilter}
                aria-label="模型筛选"
                options={modelOptions}
                onChange={setModelFilter}
              />
              <Select<StatusFilter>
                value={statusFilter}
                aria-label="状态筛选"
                options={[
                  { value: 'all', label: '全部结果' },
                  { value: 'healthy', label: '正常' },
                  { value: 'warning', label: '偏慢' },
                  { value: 'error', label: '失败' },
                ]}
                onChange={setStatusFilter}
              />
              <Input
                allowClear
                prefix={<SearchOutlined />}
                value={query}
                aria-label="搜索探测记录"
                placeholder="搜索模型或错误"
                onChange={(event) => setQuery(event.target.value)}
              />
            </>
          ) : (
            <span className={styles.usageFilterNote}>仅汇总响应中明确返回的 usage 字段</span>
          )}
        </div>

        {mode === 'records' ? (
          <div className={styles.recordsTableWrap}>
            <Table<ProbeRecord>
              className={styles.recordsTable}
              rowKey="id"
              columns={recordColumns}
              dataSource={filteredRecords}
              size="small"
              tableLayout="fixed"
              scroll={{ x: 1110 }}
              showSorterTooltip={{ target: 'sorter-icon' }}
              pagination={{
                pageSize: 10,
                showSizeChanger: false,
                showTotal: (total) => `共 ${total} 条`,
              }}
              locale={{ emptyText: '没有符合条件的探测记录' }}
            />
          </div>
        ) : (
          <div className={styles.usageView}>
            <div className={styles.usageSummaryBar}>
              <span>
                <strong>{usageRecords.length}</strong> 次探测
              </span>
              <span>
                <strong>{reportedCount}</strong> 次上报用量
              </span>
              <span>
                <strong>{formatTokens(totalTokens)}</strong> 总 Token
              </span>
              <span>
                <strong>{unavailableCount}</strong> 次未上报
              </span>
            </div>
            <Table<SiteUsageSummary>
              className={styles.usageTable}
              rowKey="siteId"
              columns={usageColumns}
              dataSource={usageSummary}
              size="small"
              tableLayout="fixed"
              scroll={{ x: 1050 }}
              pagination={false}
              showSorterTooltip={{ target: 'sorter-icon' }}
              locale={{ emptyText: '当前周期没有可统计的探测记录' }}
            />
            <p className={styles.usageFootnote}>
              未上报的请求不参与 Token 合计与平均值；RelayPulse 不根据文本长度估算用量。
            </p>
          </div>
        )}
      </main>

      {selectedRecord ? (
        <RecordDetailDrawer record={selectedRecord} onClose={() => setSelectedRecord(null)} />
      ) : null}

      {clearConfirmOpen ? (
        <Modal
          open
          title="清理全部探测记录？"
          width={440}
          centered
          getContainer={overlayContainer}
          rootClassName={styles.scopedOverlay}
          rootStyle={{ position: 'absolute' }}
          mask={{ closable: false }}
          okText="清理记录"
          okButtonProps={{ danger: true }}
          confirmLoading={clearing}
          cancelText="取消"
          onCancel={() => setClearConfirmOpen(false)}
          onOk={() => void clearRecords()}
        >
          <p className={styles.modalLead}>
            将删除本机保存的全部探测记录和 Token 统计数据，站点与模型配置不会受到影响。
          </p>
        </Modal>
      ) : null}
    </>
  );
}
