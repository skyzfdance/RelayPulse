import {
  DeleteOutlined,
  FileSearchOutlined,
  MoreOutlined,
  ReloadOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import {
  Button,
  Collapse,
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
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { StatusDot } from '../../components/StatusDot';
import type {
  ProbeCapture,
  ProbeRecord,
  ProbeRecordStatus,
  RequestHeader,
} from '../../domain/types';
import { pageHeaderClass, scopedOverlayClass } from '../../styles/tailwind';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

const drawerSectionClass =
  'border-b border-rp-line py-4 last:border-b-0 [&>h3]:m-0 [&>h3]:text-sm [&>h3]:leading-6 [&>h3]:font-semibold';

const recordDrawerClass = `${scopedOverlayClass} [&_.ant-drawer-body]:scrollbar-none [&_.ant-drawer-body]:[-ms-overflow-style:none] [&_.ant-drawer-body::-webkit-scrollbar]:hidden`;

const tableStructureClass =
  'relative flex h-full min-h-0 flex-col overflow-hidden text-rp [&>.ant-spin]:absolute [&>.ant-spin]:inset-0 [&>.ant-spin]:overflow-hidden [&_.ant-spin-nested-loading]:h-full [&_.ant-spin-nested-loading]:min-h-0 [&_.ant-spin-container]:flex [&_.ant-spin-container]:h-full [&_.ant-spin-container]:min-h-0 [&_.ant-spin-container]:flex-1 [&_.ant-spin-container]:flex-col [&_.ant-spin-container]:overflow-hidden [&_.ant-table]:flex [&_.ant-table]:min-h-0 [&_.ant-table]:flex-1 [&_.ant-table]:flex-col [&_.ant-table]:overflow-hidden [&_.ant-table]:text-rp [&_.ant-table-container]:flex [&_.ant-table-container]:min-h-0 [&_.ant-table-container]:flex-1 [&_.ant-table-container]:flex-col [&_.ant-table-container]:overflow-hidden [&_.ant-table-header]:flex-none [&_.ant-table-body]:min-h-0 [&_.ant-table-body]:max-h-full! [&_.ant-table-body]:flex-1 [&_.ant-table-body]:overflow-y-auto! [&_.ant-table-body]:scrollbar-none [&_.ant-table-body]:[-ms-overflow-style:none] [&_.ant-table-body::-webkit-scrollbar]:hidden [&_.ant-table-thead>tr>th]:bg-rp-subtle [&_.ant-table-thead>tr>th]:text-xs [&_.ant-table-thead>tr>th]:font-normal [&_.ant-table-thead>tr>th]:text-rp-secondary [&_.ant-table-cell]:[font-variant-numeric:tabular-nums]';

const recordsTableClass = `${tableStructureClass} absolute inset-0 [&_.ant-pagination]:my-3! [&_.ant-pagination]:flex-none`;
const usageTableClass = `${tableStructureClass} flex-1`;

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

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  const kilobytes = value / 1024;
  return `${kilobytes.toFixed(kilobytes >= 10 ? 0 : 1)} KB`;
}

function CaptureHeaders({ headers }: { headers: RequestHeader[] }) {
  if (headers.length === 0) {
    return <p className="m-0 text-xs leading-5 text-rp-tertiary">未收到响应头</p>;
  }

  return (
    <dl className="m-0 [&>div]:grid [&>div]:min-w-0 [&>div]:grid-cols-[118px_minmax(0,1fr)] [&>div]:gap-2 [&>div]:border-b [&>div]:border-rp-line [&>div]:py-1.25 [&>div]:text-[11px] [&>div]:leading-4.5 [&>div:last-child]:border-b-0 [&_dd]:m-0 [&_dd]:min-w-0 [&_dd]:wrap-anywhere [&_dt]:text-rp-tertiary">
      {headers.map((header, index) => (
        <div key={`${header.key}-${index}`}>
          <dt>{header.key}</dt>
          <dd className="font-mono text-[13px]">{header.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function CaptureBody({ capture, emptyText }: { capture: ProbeCapture; emptyText: string }) {
  return (
    <>
      {capture.rawBody ? (
        <pre className="m-0 max-h-55 overflow-auto rounded-md bg-[#141414] px-3 py-2.5 font-mono text-[11px] leading-4.5 whitespace-pre-wrap text-white wrap-anywhere scrollbar-none [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden">
          {capture.rawBody}
        </pre>
      ) : (
        <p className="m-0 text-xs leading-5 text-rp-tertiary">{emptyText}</p>
      )}
      {capture.truncated ? (
        <p className="m-0 text-xs leading-5 text-rp-warning-text">
          内容超过保存上限，仅记录前 {formatBytes(capture.captureLimitBytes)}。
        </p>
      ) : null}
    </>
  );
}

function RecordStatus({ status }: { status: ProbeRecordStatus }) {
  return (
    <span className="inline-flex items-center gap-1.75 whitespace-nowrap">
      <StatusDot status={status} />
      {statusMeta[status].label}
    </span>
  );
}

function TokenValue({ record }: { record: ProbeRecord }) {
  if (record.tokenUsage.source === 'unavailable') {
    return (
      <Tooltip title="供应商未返回 usage">
        <span className="text-rp-secondary">—</span>
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
  const { request, response } = record.exchange;
  const [open, setOpen] = useState(true);
  return (
    <Drawer
      open={open}
      size={520}
      closable={{ placement: 'end' }}
      title={
        <div>
          <strong>探测详情</strong>
          <div className="mt-0.5 text-xs font-normal text-rp-tertiary">
            {record.modelName} · {record.siteName}
          </div>
        </div>
      }
      getContainer={overlayContainer}
      rootClassName={recordDrawerClass}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: true }}
      onClose={() => setOpen(false)}
      afterOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <div className="border-b border-rp-line pb-3.5">
        <div className="mb-2 flex items-center justify-between gap-4 text-xs text-rp-tertiary">
          <RecordStatus status={record.status} />
          <span>{formatTimestamp(record.startedAt)}</span>
        </div>
        <strong className="text-base leading-6">{record.result}</strong>
      </div>

      <section className={`${drawerSectionClass} pt-3.5`}>
        <h3>性能与用量</h3>
        <dl className="mt-2.5 grid grid-cols-3 gap-x-3 gap-y-4 [&>div]:min-w-0 [&_dd]:m-0 [&_dd]:text-rp [&_dd]:[font-variant-numeric:tabular-nums] [&_dt]:mb-1 [&_dt]:text-xs [&_dt]:text-rp-tertiary">
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

      <section className={drawerSectionClass}>
        <h3>请求信息</h3>
        <dl className="mt-2 flex flex-col [&>div]:grid [&>div]:min-h-12 [&>div]:grid-cols-[100px_minmax(0,1fr)] [&>div]:border-b [&>div]:border-rp-line [&>div]:py-2 [&>div:last-child]:border-b-0 [&_dd]:m-0 [&_dd]:min-w-0 [&_dd]:wrap-anywhere [&_dt]:text-rp-tertiary">
          <div>
            <dt>请求端点</dt>
            <dd className="font-mono text-[13px]">{record.endpoint}</dd>
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
            <dd className="font-mono text-[13px]">{record.requestId ?? '—'}</dd>
          </div>
        </dl>
      </section>

      <section className={drawerSectionClass}>
        <h3>请求与响应</h3>
        <Collapse
          size="small"
          className="mt-2 bg-rp-surface [&_.ant-collapse-content-box]:px-3 [&_.ant-collapse-content-box]:pt-1 [&_.ant-collapse-content-box]:pb-3 [&_.ant-collapse-header]:items-center! [&_.ant-collapse-header-text]:font-semibold"
          expandIconPlacement="end"
          items={[
            {
              key: 'request',
              label: '请求详情',
              extra: (
                <span className="inline-flex items-center gap-1.5 text-xs font-normal text-rp-tertiary [font-variant-numeric:tabular-nums]">
                  {record.method} · {formatBytes(request.capturedBytes)}
                </span>
              ),
              children: (
                <div className="flex flex-col gap-3">
                  <div className="flex min-w-0 flex-col gap-1.5">
                    <span className="text-xs font-medium text-rp-secondary">
                      请求头（敏感字段已脱敏）
                    </span>
                    <CaptureHeaders headers={request.headers} />
                  </div>
                  <div className="flex min-w-0 flex-col gap-1.5">
                    <span className="text-xs font-medium text-rp-secondary">原始请求体</span>
                    <CaptureBody capture={request} emptyText="没有请求正文" />
                  </div>
                </div>
              ),
            },
            {
              key: 'response',
              label: '原始返回',
              extra: (
                <span className="inline-flex items-center gap-1.5 text-xs font-normal text-rp-tertiary [font-variant-numeric:tabular-nums] [&_.ant-tag]:m-0">
                  <Tag color={response.mode === 'stream' ? 'blue' : undefined}>
                    {response.mode === 'stream' ? '流式' : '同步'}
                  </Tag>
                  {formatBytes(response.capturedBytes)}
                </span>
              ),
              children: (
                <div className="flex flex-col gap-3">
                  <div className="flex min-w-0 flex-col gap-1.5">
                    <span className="text-xs font-medium text-rp-secondary">响应头</span>
                    <CaptureHeaders headers={response.headers} />
                  </div>
                  {response.mode === 'stream' && response.chunks.length > 0 ? (
                    <div className="flex min-w-0 flex-col gap-1.5">
                      <span className="text-xs font-medium text-rp-secondary">流式分片</span>
                      <div className="flex flex-wrap gap-1.5">
                        {response.chunks.map((chunk) => (
                          <span
                            className="inline-flex items-baseline gap-1.5 rounded border border-rp-line bg-rp-subtle px-2 py-1.25 text-[11px] [font-variant-numeric:tabular-nums] [&_small]:text-[10px] [&_small]:text-rp-tertiary"
                            key={chunk.sequence}
                          >
                            分片 {chunk.sequence}
                            <small>
                              +{chunk.receivedAfterMs} ms · {formatBytes(chunk.capturedBytes)}
                            </small>
                          </span>
                        ))}
                      </div>
                    </div>
                  ) : null}
                  <div className="flex min-w-0 flex-col gap-1.5">
                    <span className="text-xs font-medium text-rp-secondary">
                      {response.mode === 'stream' ? '合并后的原始流' : '原始响应体'}
                    </span>
                    <CaptureBody capture={response} emptyText="未收到响应正文" />
                  </div>
                </div>
              ),
            },
          ]}
        />
      </section>

      {record.errorDetail ? (
        <section className={drawerSectionClass}>
          <h3>错误信息</h3>
          <div className="mt-2 rounded-md border border-rp-error bg-rp-error-bg px-3 py-2.5 leading-5.5 text-rp-error">
            {record.errorDetail}
          </div>
        </section>
      ) : null}
    </Drawer>
  );
}

export function RecordsPage() {
  const { snapshot, refreshProbeRecords, clearProbeRecords } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
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
        <span className="whitespace-nowrap text-rp-secondary [font-variant-numeric:tabular-nums]">
          {formatTimestamp(value)}
        </span>
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
        <span className="font-medium" title={value}>
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
        <span className={record.status === 'error' ? 'text-rp-error' : undefined} title={value}>
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
            className="text-rp-primary!"
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
      await runBackendAction(() => refreshProbeRecords(), {
        key: 'refresh-probe-records',
        presentation: 'message',
        successTitle: '探测记录已刷新',
        successDescription: '当前列表已更新为后端返回的最新记录。',
        errorTitle: '探测记录刷新失败',
        errorDescription: '仍显示上一次记录，请稍后重试。',
      });
    } finally {
      setRefreshing(false);
    }
  }

  async function clearRecords() {
    setClearing(true);
    try {
      const cleared = await runBackendAction(() => clearProbeRecords(), {
        key: 'clear-probe-records',
        successTitle: '探测记录已清理',
        successDescription: '本地探测记录和 Token 统计数据已全部删除。',
        errorTitle: '探测记录清理失败',
        errorDescription: '记录没有删除，请稍后重试。',
      });
      if (cleared) {
        setSelectedRecord(null);
        setClearConfirmOpen(false);
      }
    } finally {
      setClearing(false);
    }
  }

  return (
    <>
      <header className={pageHeaderClass}>
        <div>
          <h1>探测记录</h1>
          <p>查看模型可用性、响应耗时和供应商上报的 Token 用量</p>
        </div>
        <div className="flex gap-2">
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

      <main className="flex min-h-0 flex-1 flex-col overflow-hidden px-6 pt-4">
        <div className="mb-3 flex min-h-8 items-center justify-between gap-4 [&>span]:text-xs [&>span]:text-rp-tertiary">
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

        <div className="flex min-h-11 flex-wrap items-start gap-2 border-b border-rp-line pb-3 [&>.ant-input-affix-wrapper]:ml-auto [&>.ant-input-affix-wrapper]:w-55 [&>.ant-select]:w-36 [&_.anticon-search]:text-rp-quaternary">
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
            <span className="ml-auto pt-1.5 text-xs text-rp-tertiary">
              仅汇总响应中明确返回的 usage 字段
            </span>
          )}
        </div>

        {mode === 'records' ? (
          <div className="relative min-h-0 flex-1 overflow-hidden">
            <Table<ProbeRecord>
              className={recordsTableClass}
              rowKey="id"
              columns={recordColumns}
              dataSource={filteredRecords}
              size="small"
              tableLayout="fixed"
              scroll={{ x: 1110, y: '100%' }}
              showSorterTooltip={{ target: 'sorter-icon' }}
              pagination={{
                defaultPageSize: 10,
                pageSizeOptions: [10, 20, 50, 100],
                responsive: true,
                showQuickJumper: true,
                showSizeChanger: true,
                showTotal: (total, range) => `第 ${range[0]}-${range[1]} 条，共 ${total} 条`,
              }}
              locale={{ emptyText: '没有符合条件的探测记录' }}
            />
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <div className="flex min-h-13 flex-none items-center border-b border-rp-line bg-rp-subtle [&>span]:min-w-36.25 [&>span]:border-r [&>span]:border-rp-line [&>span]:px-5 [&>span]:text-xs [&>span]:text-rp-secondary [&>span:last-child]:border-r-0 [&_strong]:mr-1 [&_strong]:text-base [&_strong]:font-semibold [&_strong]:text-rp [&_strong]:[font-variant-numeric:tabular-nums]">
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
              className={usageTableClass}
              rowKey="siteId"
              columns={usageColumns}
              dataSource={usageSummary}
              size="small"
              tableLayout="fixed"
              scroll={{ x: 1050, y: '100%' }}
              pagination={false}
              showSorterTooltip={{ target: 'sorter-icon' }}
              locale={{ emptyText: '当前周期没有可统计的探测记录' }}
            />
            <p className="mt-3 flex-none text-xs leading-5 text-rp-tertiary">
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
          rootClassName={scopedOverlayClass}
          rootStyle={{ position: 'absolute' }}
          mask={{ closable: false }}
          okText="清理记录"
          okButtonProps={{ danger: true }}
          confirmLoading={clearing}
          cancelText="取消"
          onCancel={() => setClearConfirmOpen(false)}
          onOk={() => void clearRecords()}
        >
          <p className="m-0 leading-5.5 text-rp-secondary">
            将删除本机保存的全部探测记录和 Token 统计数据，站点与模型配置不会受到影响。
          </p>
        </Modal>
      ) : null}
    </>
  );
}
