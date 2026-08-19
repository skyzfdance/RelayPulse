import {
  ArrowRightOutlined,
  ClearOutlined,
  FileSearchOutlined,
  ReloadOutlined,
  SearchOutlined,
} from '@ant-design/icons';
import {
  Alert,
  Button,
  Collapse,
  DatePicker,
  Descriptions,
  Drawer,
  Empty,
  Input,
  Result,
  Select,
  Skeleton,
  Table,
  Tag,
  Tooltip,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { recordFrontendAction } from '../../app/frontend-diagnostics';
import { navigateToView } from '../../app/view';
import { formatLocalTimestamp } from '../../domain/time';
import { pageHeaderClass } from '../../styles/tailwind';
import type {
  DataCollectionDetails,
  DiagnosticsDetails,
  LifecycleDetails,
  OperationCategory,
  OperationHistoryDataSource,
  OperationOccurredAt,
  OperationRecordDetails,
  OperationRecordSummary,
  OperationTrigger,
} from './operation-history-types';
import { OperationHistoryError } from './operation-history-types';

const { RangePicker } = DatePicker;
const PAGE_SIZE = 10;
const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
const drawerSectionClass =
  'border-b border-rp-line py-4 last:border-b-0 [&>h3]:m-0 [&>h3]:text-sm [&>h3]:leading-6 [&>h3]:font-semibold';

const categoryMeta: Record<OperationCategory, { label: string; color: string }> = {
  app_lifecycle: { label: '应用运行', color: 'blue' },
  configuration_change: { label: '配置变更', color: 'cyan' },
  data_collection: { label: '数据采集', color: 'green' },
  diagnostics_maintenance: { label: '诊断维护', color: 'gold' },
};

const triggerLabels: Record<OperationTrigger, string> = {
  user_action: '用户操作',
  automatic: '自动执行',
  system_startup: '系统启动',
  system_recovery: '系统恢复',
  system_shutdown: '系统关闭',
};

function formatOccurredAt(occurredAt: OperationOccurredAt) {
  const formatted = formatLocalTimestamp(occurredAt.value);
  if (occurredAt.precision === 'exact') return formatted;
  return occurredAt.qualifier === 'after' ? `${formatted} 后` : `约 ${formatted}`;
}

function booleanLabel(value: boolean) {
  return value ? '是' : '否';
}

function DetailRows({ rows }: { rows: { label: string; value: string | number }[] }) {
  return (
    <dl className="mt-2 flex flex-col [&>div]:grid [&>div]:min-h-10 [&>div]:grid-cols-[128px_minmax(0,1fr)] [&>div]:items-start [&>div]:border-b [&>div]:border-rp-line [&>div]:py-2 [&>div:last-child]:border-b-0 [&_dd]:m-0 [&_dd]:min-w-0 [&_dd]:wrap-anywhere [&_dt]:text-rp-tertiary">
      {rows.map((row) => (
        <div key={row.label}>
          <dt>{row.label}</dt>
          <dd>{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ConfigurationDetails({ details }: { details: OperationRecordDetails }) {
  if (details.detail.kind !== 'configuration_change') return null;
  return (
    <section className={drawerSectionClass}>
      <h3>变更内容</h3>
      <div className="mt-2 overflow-hidden rounded-md border border-rp-line">
        <table className="w-full table-fixed border-collapse text-left text-xs">
          <thead className="bg-rp-subtle text-rp-secondary">
            <tr>
              <th className="w-[32%] px-3 py-2 font-medium">字段</th>
              <th className="w-[34%] px-3 py-2 font-medium">修改前</th>
              <th className="w-[34%] px-3 py-2 font-medium">修改后</th>
            </tr>
          </thead>
          <tbody className="[&_td]:border-t [&_td]:border-rp-line [&_td]:px-3 [&_td]:py-2 [&_td]:align-top [&_td]:wrap-anywhere">
            {details.detail.changes.map((change) => (
              <tr key={change.fieldLabel}>
                <td className="font-medium">{change.fieldLabel}</td>
                <td className="text-rp-secondary">{change.before ?? '未提交'}</td>
                <td>{change.after ?? '未提交'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function LifecycleDetailSection({ detail }: { detail: LifecycleDetails }) {
  const rows: { label: string; value: string | number }[] = [];
  if (detail.launchMethod) rows.push({ label: '启动方式', value: detail.launchMethod });
  if (detail.appVersion) rows.push({ label: '应用版本', value: detail.appVersion });
  if (detail.restoredSiteCount !== undefined)
    rows.push({ label: '恢复站点', value: `${detail.restoredSiteCount} 个` });
  if (detail.restoredModelCount !== undefined)
    rows.push({ label: '恢复模型', value: `${detail.restoredModelCount} 个` });
  if (detail.backgroundTasksStarted !== undefined)
    rows.push({ label: '后台任务已启动', value: booleanLabel(detail.backgroundTasksStarted) });
  if (detail.settingsSaved !== undefined)
    rows.push({ label: '配置写入完成', value: booleanLabel(detail.settingsSaved) });
  if (detail.windowStateSaved !== undefined)
    rows.push({ label: '窗口状态已保存', value: booleanLabel(detail.windowStateSaved) });
  if (detail.backgroundTasksStopped !== undefined)
    rows.push({ label: '后台任务已停止', value: booleanLabel(detail.backgroundTasksStopped) });
  if (detail.previousSessionStartedAt)
    rows.push({
      label: '上次会话开始',
      value: formatLocalTimestamp(detail.previousSessionStartedAt),
    });
  if (detail.lastCompletedAction)
    rows.push({ label: '最后完成操作', value: detail.lastCompletedAction });
  if (detail.failureSource) rows.push({ label: '异常来源', value: detail.failureSource });
  if (detail.crashReportFound !== undefined)
    rows.push({ label: '系统崩溃报告', value: detail.crashReportFound ? '已找到' : '未找到' });

  return (
    <section className={drawerSectionClass}>
      <h3>{detail.event === 'abnormal_exit' ? '异常退出识别' : '运行详情'}</h3>
      <DetailRows rows={rows} />
    </section>
  );
}

function CollectionDetailSection({ detail }: { detail: DataCollectionDetails }) {
  const rows: { label: string; value: string | number }[] = [
    { label: '数据来源', value: '智力天梯' },
  ];
  if (detail.entryCount !== undefined) {
    rows.push({ label: '采集条目', value: `${detail.entryCount} 个` });
  }
  return (
    <section className={drawerSectionClass}>
      <h3>采集摘要</h3>
      <DetailRows rows={rows} />
      <Button
        type="link"
        className="mt-2 px-0!"
        icon={<ArrowRightOutlined />}
        iconPlacement="end"
        onClick={() => navigateToView('intelligence-ladder')}
      >
        查看智力天梯
      </Button>
    </section>
  );
}

function DiagnosticsDetailSection({ detail }: { detail: DiagnosticsDetails }) {
  const rows =
    detail.action === 'export'
      ? [
          { label: '安全数据类别', value: detail.includedCategories.join('、') },
          { label: '日志文件', value: `${detail.fileCount} 个` },
          { label: '诊断包大小', value: detail.bundleSizeLabel },
          { label: '保存位置', value: detail.locationLabel },
          {
            label: '请求与响应正文',
            value: detail.requestBodiesExcluded ? '未包含' : '包含安全摘要',
          },
        ]
      : [
          { label: '清理前大小', value: detail.beforeSizeLabel },
          { label: '清理文件', value: `${detail.fileCount} 个` },
          { label: '清理后大小', value: detail.afterSizeLabel },
          {
            label: '站点、模型与设置',
            value: detail.settingsPreserved ? '未删除' : '状态未知',
          },
        ];
  return (
    <section className={drawerSectionClass}>
      <h3>{detail.action === 'export' ? '导出内容' : '清理结果'}</h3>
      <DetailRows rows={rows} />
    </section>
  );
}

function SpecificDetailSection({ details }: { details: OperationRecordDetails }) {
  switch (details.detail.kind) {
    case 'configuration_change':
      return <ConfigurationDetails details={details} />;
    case 'app_lifecycle':
      return <LifecycleDetailSection detail={details.detail} />;
    case 'data_collection':
      return <CollectionDetailSection detail={details.detail} />;
    case 'diagnostics_maintenance':
      return <DiagnosticsDetailSection detail={details.detail} />;
  }
}

function OperationDetailContent({ details }: { details: OperationRecordDetails }) {
  const basicItems = [
    { key: 'time', label: '发生时间', children: formatOccurredAt(details.occurredAt) },
    { key: 'category', label: '类型', children: categoryMeta[details.category].label },
    { key: 'trigger', label: '触发方式', children: triggerLabels[details.trigger] },
    ...(details.subject ? [{ key: 'subject', label: '对象', children: details.subject }] : []),
  ];
  return (
    <>
      <div className="border-b border-rp-line pb-4">
        <p className="m-0 text-sm leading-6 text-rp-secondary">{details.description}</p>
      </div>
      <section className={drawerSectionClass}>
        <h3>基本信息</h3>
        <Descriptions className="mt-2" column={1} colon={false} size="small" items={basicItems} />
      </section>
      <SpecificDetailSection details={details} />
      {details.exception ? (
        <section className={drawerSectionClass}>
          <h3>异常信息</h3>
          <div className="mt-3 flex flex-col gap-4">
            <Alert
              type="warning"
              showIcon
              title={details.exception.reason}
              description={details.exception.recommendedAction}
            />
            {details.exception.lastStep || details.exception.safeCode ? (
              <Collapse
                size="small"
                expandIconPlacement="end"
                items={[
                  {
                    key: 'technical-details',
                    label: '更多诊断信息',
                    children: (
                      <DetailRows
                        rows={[
                          ...(details.exception?.lastStep
                            ? [{ label: '最后语义步骤', value: details.exception.lastStep }]
                            : []),
                          ...(details.exception?.safeCode
                            ? [{ label: '安全错误码', value: details.exception.safeCode }]
                            : []),
                        ]}
                      />
                    ),
                  },
                ]}
              />
            ) : null}
          </div>
        </section>
      ) : null}
    </>
  );
}

function OperationDetailDrawer({
  dataSource,
  record,
  onClosed,
}: {
  dataSource: OperationHistoryDataSource;
  record: OperationRecordSummary;
  onClosed(): void;
}) {
  const [open, setOpen] = useState(true);
  const [details, setDetails] = useState<OperationRecordDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [requestVersion, setRequestVersion] = useState(0);

  useEffect(() => {
    let disposed = false;
    void dataSource
      .getRecordDetails(record.recordRef)
      .then((result) => {
        if (!disposed) setDetails(result);
      })
      .catch(() => {
        if (!disposed) setError('操作详情加载失败，请稍后重试。');
      });
    return () => {
      disposed = true;
    };
  }, [dataSource, record.recordRef, requestVersion]);

  return (
    <Drawer
      open={open}
      size={520}
      closable={{ placement: 'end' }}
      title={
        <div>
          <strong>{record.actionLabel}</strong>
          <div className="mt-0.5 text-xs font-normal text-rp-tertiary">
            {categoryMeta[record.category].label} · {formatOccurredAt(record.occurredAt)}
          </div>
        </div>
      }
      getContainer={overlayContainer}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: true }}
      onClose={() => setOpen(false)}
      afterOpenChange={(nextOpen) => {
        if (!nextOpen) onClosed();
      }}
    >
      {!details && !error ? (
        <Skeleton active paragraph={{ rows: 8 }} />
      ) : error ? (
        <Result
          status="warning"
          title="操作详情加载失败"
          subTitle={error}
          extra={
            <Button
              onClick={() => {
                setDetails(null);
                setError(null);
                setRequestVersion((current) => current + 1);
              }}
            >
              重试
            </Button>
          }
        />
      ) : details ? (
        <OperationDetailContent details={details} />
      ) : null}
    </Drawer>
  );
}

function errorMessage(cause: unknown) {
  if (cause instanceof OperationHistoryError) return cause.message;
  return '操作记录查询失败，请稍后重试。';
}

interface OperationRecordRow extends OperationRecordSummary {
  uiKey: string;
}

export function OperationHistoryPage({ dataSource }: { dataSource: OperationHistoryDataSource }) {
  const [records, setRecords] = useState<OperationRecordRow[]>([]);
  const [nextCursor, setNextCursor] = useState<string | undefined>();
  const [category, setCategory] = useState<OperationCategory | undefined>();
  const [startedAfter, setStartedAfter] = useState<string | undefined>();
  const [startedBefore, setStartedBefore] = useState<string | undefined>();
  const [searchInput, setSearchInput] = useState('');
  const [operationQuery, setOperationQuery] = useState('');
  const [pickerVersion, setPickerVersion] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [selectedRecord, setSelectedRecord] = useState<OperationRecordSummary | null>(null);
  const latestRequest = useRef(0);
  const rowKeySequence = useRef(0);
  const datasetVersion = useRef<string | null>(null);
  const detailTrigger = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => setOperationQuery(searchInput.trim()), 300);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  const queryBase = useMemo(
    () => ({
      limit: PAGE_SIZE,
      ...(startedAfter ? { startedAfter } : {}),
      ...(startedBefore ? { startedBefore } : {}),
      ...(category ? { category } : {}),
      ...(operationQuery ? { operationQuery } : {}),
    }),
    [category, operationQuery, startedAfter, startedBefore],
  );

  const loadFirstPage = useCallback(async () => {
    const requestId = latestRequest.current + 1;
    latestRequest.current = requestId;
    setLoading(true);
    setLoadError(null);
    try {
      const page = await dataSource.listRecords(queryBase);
      if (latestRequest.current !== requestId) return;
      rowKeySequence.current = 0;
      setRecords(
        page.items.map((record) => ({
          ...record,
          uiKey: `operation-row-${rowKeySequence.current++}`,
        })),
      );
      setNextCursor(page.nextCursor);
      datasetVersion.current = page.datasetVersion;
      setSelectedRecord(null);
    } catch (cause) {
      if (latestRequest.current !== requestId) return;
      setLoadError(errorMessage(cause));
    } finally {
      if (latestRequest.current === requestId) setLoading(false);
    }
  }, [dataSource, queryBase]);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadFirstPage(), 0);
    return () => window.clearTimeout(timer);
  }, [loadFirstPage]);

  async function loadMore() {
    if (!nextCursor || loadingMore) return;
    const requestId = latestRequest.current;
    recordFrontendAction('load-more-operation-records');
    setLoadingMore(true);
    setLoadError(null);
    try {
      const page = await dataSource.listRecords({ ...queryBase, cursor: nextCursor });
      if (latestRequest.current !== requestId) return;
      if (datasetVersion.current !== page.datasetVersion) {
        setNotice('日志内容已变化，已重新载入最新记录。');
        await loadFirstPage();
        return;
      }
      const appendedRows = page.items.map((record) => ({
        ...record,
        uiKey: `operation-row-${rowKeySequence.current++}`,
      }));
      setRecords((current) => [...current, ...appendedRows]);
      setNextCursor(page.nextCursor);
    } catch (cause) {
      if (latestRequest.current !== requestId) return;
      if (cause instanceof OperationHistoryError && cause.code === 'cursor_expired') {
        setNotice('日志内容已变化，已重新载入最新记录。');
        await loadFirstPage();
      } else {
        setLoadError(errorMessage(cause));
      }
    } finally {
      setLoadingMore(false);
    }
  }

  function invalidateCurrentQuery() {
    latestRequest.current += 1;
    setNextCursor(undefined);
  }

  function resetFilters() {
    recordFrontendAction('operation-history-filter-changed');
    invalidateCurrentQuery();
    setCategory(undefined);
    setStartedAfter(undefined);
    setStartedBefore(undefined);
    setSearchInput('');
    setOperationQuery('');
    setPickerVersion((current) => current + 1);
  }

  const filtered = Boolean(category || startedAfter || startedBefore || operationQuery);
  const columns: ColumnsType<OperationRecordRow> = [
    {
      title: '时间',
      dataIndex: 'occurredAt',
      width: 190,
      render: (occurredAt: OperationOccurredAt) => (
        <span className="whitespace-nowrap [font-variant-numeric:tabular-nums]">
          {formatOccurredAt(occurredAt)}
        </span>
      ),
    },
    {
      title: '类型',
      dataIndex: 'category',
      width: 128,
      render: (value: OperationCategory) => (
        <Tag className="m-0" color={categoryMeta[value].color}>
          {categoryMeta[value].label}
        </Tag>
      ),
    },
    {
      title: '操作',
      dataIndex: 'actionLabel',
      ellipsis: { showTitle: false },
      render: (value: string) => (
        <span className="font-medium" title={value}>
          {value}
        </span>
      ),
    },
    {
      title: '详情',
      key: 'action',
      width: 60,
      align: 'center',
      fixed: 'right',
      render: (_, record) => (
        <Tooltip title="查看操作详情">
          <Button
            type="text"
            shape="circle"
            icon={<FileSearchOutlined />}
            aria-label={`查看 ${record.actionLabel} 操作详情`}
            className="text-rp-primary!"
            onClick={(event) => {
              recordFrontendAction('open-operation-record');
              detailTrigger.current = event.currentTarget;
              setSelectedRecord(record);
            }}
          />
        </Tooltip>
      ),
    },
  ];

  return (
    <>
      <header className={pageHeaderClass}>
        <div>
          <h1>操作记录</h1>
          <p>查看应用运行、配置变更、数据采集和诊断维护记录</p>
        </div>
        <Tooltip title="刷新操作记录">
          <Button
            shape="circle"
            icon={<ReloadOutlined />}
            loading={loading}
            aria-label="刷新操作记录"
            onClick={() => {
              recordFrontendAction('refresh-operation-records');
              setNotice(null);
              void loadFirstPage();
            }}
          />
        </Tooltip>
      </header>

      <main className="flex min-h-0 flex-1 flex-col overflow-hidden px-6 pt-4">
        <div className="flex min-h-11 flex-wrap items-start gap-2 border-b border-rp-line pb-3">
          <RangePicker
            key={pickerVersion}
            className="w-62"
            allowClear
            format="YYYY-MM-DD"
            placeholder={['开始日期', '结束日期']}
            aria-label="操作时间范围"
            onChange={(dates) => {
              recordFrontendAction('operation-history-filter-changed');
              invalidateCurrentQuery();
              setStartedAfter(dates?.[0]?.startOf('day').toISOString());
              setStartedBefore(dates?.[1]?.endOf('day').toISOString());
            }}
          />
          <Select<OperationCategory>
            className="w-36"
            allowClear
            value={category}
            placeholder="全部类型"
            aria-label="操作类型筛选"
            options={Object.entries(categoryMeta).map(([value, meta]) => ({
              value: value as OperationCategory,
              label: meta.label,
            }))}
            onChange={(value) => {
              recordFrontendAction('operation-history-filter-changed');
              invalidateCurrentQuery();
              setCategory(value);
            }}
          />
          <Input
            className="min-w-45 flex-1"
            allowClear
            prefix={<SearchOutlined className="text-rp-quaternary" />}
            maxLength={80}
            value={searchInput}
            aria-label="搜索操作记录"
            placeholder="搜索操作名称"
            onChange={(event) => {
              recordFrontendAction('operation-history-search-changed');
              invalidateCurrentQuery();
              setSearchInput(event.target.value);
            }}
          />
          {filtered || searchInput ? (
            <Button icon={<ClearOutlined />} onClick={resetFilters}>
              重置筛选
            </Button>
          ) : null}
        </div>

        {notice ? (
          <Alert
            className="mt-3 flex-none"
            type="info"
            showIcon
            title={notice}
            closable={{ onClose: () => setNotice(null) }}
          />
        ) : null}

        {loadError && records.length > 0 ? (
          <Alert
            className="mt-3 flex-none"
            type="warning"
            showIcon
            title="操作记录查询失败"
            description={`${loadError} 当前仍显示上一次加载的记录。`}
            action={<Button onClick={() => void loadFirstPage()}>重试</Button>}
          />
        ) : null}

        <div className="min-h-0 flex-1 overflow-hidden py-3">
          {loadError && records.length === 0 && !loading ? (
            <Result
              status="warning"
              title="操作记录加载失败"
              subTitle={loadError}
              extra={<Button onClick={() => void loadFirstPage()}>重试</Button>}
            />
          ) : (
            <Table<OperationRecordRow>
              rowKey="uiKey"
              columns={columns}
              dataSource={records}
              size="small"
              tableLayout="fixed"
              className="rp-table-scrollbar operation-history-table"
              loading={{ spinning: loading, description: '正在查询操作记录' }}
              scroll={{ x: 720, y: 'calc(100vh - 300px)' }}
              pagination={false}
              rowClassName={(record) =>
                record.recordRef === selectedRecord?.recordRef ? 'bg-rp-active' : ''
              }
              locale={{
                emptyText: (
                  <Empty
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    description={filtered ? '没有符合筛选条件的操作记录' : '暂无操作记录'}
                  />
                ),
              }}
            />
          )}
        </div>

        {records.length > 0 ? (
          <div className="flex min-h-13 flex-none items-center justify-center border-t border-rp-line">
            {nextCursor ? (
              <Button disabled={loading} loading={loadingMore} onClick={() => void loadMore()}>
                加载更多
              </Button>
            ) : (
              <span className="text-xs text-rp-tertiary">已加载当前查询的全部记录</span>
            )}
          </div>
        ) : null}
      </main>

      {selectedRecord ? (
        <OperationDetailDrawer
          dataSource={dataSource}
          record={selectedRecord}
          onClosed={() => {
            recordFrontendAction('close-operation-record');
            setSelectedRecord(null);
            window.requestAnimationFrame(() => detailTrigger.current?.focus());
          }}
        />
      ) : null}
    </>
  );
}
