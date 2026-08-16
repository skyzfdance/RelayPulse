import type { IntelligenceRankingEntry } from './intelligence-ladder-types';
import {
  formatIntelligenceIq,
  formatIntelligenceModelName,
} from './intelligence-ladder-presentation';

// Tabler Icons, laurel-wreath (MIT). The numeral is separate dynamic React text.
const laurelWreathPaths = [
  'M6.436 8a8.6 8.6 0 0 0 -.436 2.727c0 4.017 2.686 7.273 6 7.273s6 -3.256 6 -7.273a8.6 8.6 0 0 0 -.436 -2.727',
  'M14.5 21s-.682 -3 -2.5 -3s-2.5 3 -2.5 3',
  'M18.52 5.23c.292 1.666 -1.02 2.77 -1.02 2.77s-1.603 -.563 -1.895 -2.23c-.292 -1.666 1.02 -2.77 1.02 -2.77s1.603 .563 1.895 2.23',
  'M21.094 12.14c-1.281 1.266 -3.016 .76 -3.016 .76s-.454 -1.772 .828 -3.04c1.28 -1.266 3.016 -.76 3.016 -.76s.454 1.772 -.828 3.04',
  'M17.734 18.826c-1.5 -.575 -1.734 -2.19 -1.734 -2.19s1.267 -1.038 2.767 -.462c1.5 .575 1.733 2.19 1.733 2.19s-1.267 1.038 -2.767 .462',
  'M6.267 18.826c1.5 -.575 1.733 -2.19 1.733 -2.19s-1.267 -1.038 -2.767 -.462c-1.5 .575 -1.733 2.19 -1.733 2.19s1.267 1.038 2.767 .462',
  'M2.906 12.14c1.281 1.266 3.016 .76 3.016 .76s.454 -1.772 -.828 -3.04c-1.281 -1.265 -3.016 -.76 -3.016 -.76s-.454 1.772 .828 3.04',
  'M5.48 5.23c-.292 1.666 1.02 2.77 1.02 2.77s1.603 -.563 1.895 -2.23c.292 -1.666 -1.02 -2.77 -1.02 -2.77s-1.603 .563 -1.895 2.23',
] as const;

function PodiumWreath({ rank }: { rank: number }) {
  return (
    <svg aria-hidden="true" className="intelligence-podium-wreath" viewBox="0 0 24 24">
      <circle className="intelligence-podium-wreath__disc" cx="12" cy="11" r="4.9" />
      <g className="intelligence-podium-wreath__art">
        {laurelWreathPaths.map((path) => (
          <path d={path} key={path} />
        ))}
      </g>
      <text className="intelligence-podium-wreath__rank" x="12" y="11.25">
        {rank}
      </text>
    </svg>
  );
}

const podiumMeta = {
  1: {
    className: 'intelligence-podium-item--gold order-2',
    label: '第一名',
  },
  2: {
    className: 'intelligence-podium-item--silver order-1',
    label: '第二名',
  },
  3: {
    className: 'intelligence-podium-item--bronze order-3',
    label: '第三名',
  },
} as const;

export function IntelligencePodium({ entries }: { entries: IntelligenceRankingEntry[] }) {
  const podiumEntries = entries
    .filter((entry) => entry.rank >= 1 && entry.rank <= 3)
    .sort((left, right) => left.rank - right.rank);

  if (podiumEntries.length < 3) return null;

  return (
    <section aria-label="前三名" className="intelligence-podium">
      {podiumEntries.map((entry) => {
        const meta = podiumMeta[entry.rank as keyof typeof podiumMeta];
        return (
          <article
            aria-label={meta.label}
            className={`${meta.className} intelligence-podium-item`}
            data-testid={`podium-rank-${entry.rank}`}
            key={`${entry.rank}-${entry.model}-${entry.effort}`}
          >
            <PodiumWreath rank={entry.rank} />

            <div className="intelligence-podium-stage">
              <div aria-hidden="true" className="intelligence-podium-stage__outline">
                <span className="intelligence-podium-stage__crown" />
                <span className="intelligence-podium-stage__shoulders" />
              </div>
              <strong className="intelligence-podium-stage__model" title={entry.model}>
                {formatIntelligenceModelName(entry.model)}
              </strong>
              <span
                className="intelligence-podium-stage__score"
                data-testid={`podium-iq-${entry.rank}`}
              >
                <strong>{formatIntelligenceIq(entry.iq)}</strong>
                <small>IQ</small>
              </span>
            </div>
          </article>
        );
      })}
    </section>
  );
}
