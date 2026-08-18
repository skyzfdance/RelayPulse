import { Card, Tag } from 'antd';
import type { PointerEvent } from 'react';
import {
  formatIntelligenceIq,
  formatIntelligenceModelName,
  formatIntelligenceTableEffort,
} from './intelligence-ladder-presentation';
import type { IntelligenceRankingEntry } from './intelligence-ladder-types';

const rankingAccentClasses = [
  'intelligence-ranking-card--blue',
  'intelligence-ranking-card--gray',
  'intelligence-ranking-card--green',
  'intelligence-ranking-card--purple',
  'intelligence-ranking-card--cyan',
  'intelligence-ranking-card--orange',
  'intelligence-ranking-card--red',
] as const;

function resetCardReflection(card: HTMLElement) {
  card.style.setProperty('--ranking-pointer-x', '50%');
  card.style.setProperty('--ranking-pointer-y', '50%');
  card.style.setProperty('--ranking-tilt-x', '0deg');
  card.style.setProperty('--ranking-tilt-y', '0deg');
}

function handleCardPointerMove(event: PointerEvent<HTMLDivElement>) {
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;

  const card = event.currentTarget;
  const bounds = card.getBoundingClientRect();
  const x = Math.min(1, Math.max(0, (event.clientX - bounds.left) / bounds.width));
  const y = Math.min(1, Math.max(0, (event.clientY - bounds.top) / bounds.height));

  card.style.setProperty('--ranking-pointer-x', `${(x * 100).toFixed(2)}%`);
  card.style.setProperty('--ranking-pointer-y', `${(y * 100).toFixed(2)}%`);
  card.style.setProperty('--ranking-tilt-x', `${((0.5 - y) * 2.4).toFixed(2)}deg`);
  card.style.setProperty('--ranking-tilt-y', `${((x - 0.5) * 3.2).toFixed(2)}deg`);
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
              onPointerCancel={(event) => resetCardReflection(event.currentTarget)}
              onPointerLeave={(event) => resetCardReflection(event.currentTarget)}
              onPointerMove={handleCardPointerMove}
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
              <span aria-hidden="true" className="intelligence-ranking-card__divider" />
              <div className="intelligence-ranking-card__model">
                <strong title={entry.model}>{formatIntelligenceModelName(entry.model)}</strong>
                <Tag className="intelligence-ranking-effort" title={entry.effort} variant="filled">
                  <b>推理档位</b>
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
