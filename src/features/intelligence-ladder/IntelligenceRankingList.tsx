import { Card, Tag } from 'antd';
import {
  formatIntelligenceIq,
  formatIntelligenceModelName,
  formatIntelligenceTableEffort,
} from './intelligence-ladder-presentation';
import type { IntelligenceRankingEntry } from './intelligence-ladder-types';

const rankingAccentClasses = [
  'intelligence-ranking-card--blue',
  'intelligence-ranking-card--silver',
  'intelligence-ranking-card--green',
  'intelligence-ranking-card--violet',
  'intelligence-ranking-card--cyan',
  'intelligence-ranking-card--bronze',
  'intelligence-ranking-card--gold',
] as const;

function effortClassName(effort: string) {
  switch (effort.toLowerCase()) {
    case 'ultra':
      return 'intelligence-ranking-effort--ultra';
    case 'xhigh':
      return 'intelligence-ranking-effort--xhigh';
    case 'high':
      return 'intelligence-ranking-effort--high';
    case 'medium':
      return 'intelligence-ranking-effort--medium';
    case 'max':
      return 'intelligence-ranking-effort--max';
    default:
      return 'intelligence-ranking-effort--default';
  }
}

export function IntelligenceRankingList({ entries }: { entries: IntelligenceRankingEntry[] }) {
  const rankedEntries = entries
    .filter((entry) => entry.rank >= 4)
    .sort((left, right) => left.rank - right.rank);

  if (rankedEntries.length === 0) return null;

  return (
    <section
      aria-labelledby="intelligence-ranking-title"
      className="intelligence-ranking-panel mt-3 flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <header className="intelligence-ranking-header flex min-h-10 flex-none items-center gap-2 px-0.5">
        <h2 className="truncate text-sm leading-5 font-semibold" id="intelligence-ranking-title">
          后续排名
        </h2>
        <span aria-hidden="true" className="text-rp-quaternary">
          ·
        </span>
        <span className="flex-none text-xs text-rp-tertiary">{rankedEntries.length} 个模型</span>
      </header>

      <div
        aria-label="第四名及以后"
        className="intelligence-ranking-scroll min-h-0 flex-1 overflow-y-auto pr-1"
        data-testid="intelligence-ranking-scroll"
        role="region"
      >
        <div className="intelligence-ranking-grid">
          {rankedEntries.map((entry, index) => (
            <Card
              aria-label={`第 ${entry.rank} 名，${entry.model}`}
              className={`${rankingAccentClasses[index % rankingAccentClasses.length]} intelligence-ranking-card`}
              classNames={{ body: 'intelligence-ranking-card__body' }}
              data-testid={`ranking-entry-${entry.rank}`}
              hoverable
              key={`${entry.rank}-${entry.model}-${entry.effort}`}
              role="article"
              size="small"
              variant="outlined"
            >
              <div className="intelligence-ranking-card__rank">
                <strong aria-label={`第 ${entry.rank} 名`}>
                  {String(entry.rank).padStart(2, '0')}
                </strong>
                <span aria-hidden="true" />
              </div>
              <div className="min-w-0">
                <strong
                  className="block truncate text-base leading-6 font-semibold"
                  title={entry.model}
                >
                  {formatIntelligenceModelName(entry.model)}
                </strong>
                <Tag
                  className={`${effortClassName(entry.effort)} intelligence-ranking-effort mt-2 mb-0 max-w-full truncate rounded-sm px-2.5 text-center text-xs leading-6 font-medium`}
                  title={entry.effort}
                  variant="filled"
                >
                  {formatIntelligenceTableEffort(entry.effort)}
                </Tag>
              </div>
              <div className="intelligence-ranking-card__score">
                <strong>{formatIntelligenceIq(entry.iq)}</strong>
                <span>IQ</span>
              </div>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
