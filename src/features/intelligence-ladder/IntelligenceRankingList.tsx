import { Table } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import type { IntelligenceRankingEntry } from './intelligence-ladder-types';

function formatIq(value: number) {
  return new Intl.NumberFormat('zh-CN', {
    minimumFractionDigits: 1,
    maximumFractionDigits: 2,
  }).format(value);
}

const columns: ColumnsType<IntelligenceRankingEntry> = [
  {
    title: '排名',
    dataIndex: 'rank',
    width: 90,
    render: (rank: number) => (
      <strong className="[font-variant-numeric:tabular-nums]">{rank}</strong>
    ),
  },
  {
    title: '模型',
    dataIndex: 'model',
    minWidth: 220,
    ellipsis: { showTitle: false },
    render: (model: string) => (
      <span className="font-medium" title={model}>
        {model}
      </span>
    ),
  },
  {
    title: '努力档位',
    dataIndex: 'effort',
    width: 180,
    ellipsis: { showTitle: false },
    render: (effort: string) => <span title={effort}>{effort}</span>,
  },
  {
    title: '智力分数 (IQ)',
    dataIndex: 'iq',
    width: 150,
    align: 'right',
    render: (iq: number) => (
      <span className="font-medium [font-variant-numeric:tabular-nums]">{formatIq(iq)}</span>
    ),
  },
];

export function IntelligenceRankingList({ entries }: { entries: IntelligenceRankingEntry[] }) {
  const rankedEntries = entries
    .filter((entry) => entry.rank >= 4)
    .sort((left, right) => left.rank - right.rank);

  if (rankedEntries.length === 0) return null;

  return (
    <section aria-label="第四名及以后" className="mt-3 min-w-0">
      <Table<IntelligenceRankingEntry>
        className="rp-table-scrollbar intelligence-ladder-table"
        columns={columns}
        dataSource={rankedEntries}
        pagination={false}
        rowKey={(entry) => `${entry.rank}-${entry.model}-${entry.effort}`}
        scroll={{ x: 640 }}
        size="small"
        tableLayout="fixed"
      />
    </section>
  );
}
