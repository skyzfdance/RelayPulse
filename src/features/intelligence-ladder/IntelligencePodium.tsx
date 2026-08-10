import type { IntelligenceRankingEntry } from './intelligence-ladder-types';

const podiumMeta = {
  1: {
    order: 'order-2 min-h-55 border-t-[#d6a719]',
    badge: 'border-[#e2b72d] bg-[#f7d96b] text-[#7a5600]',
    label: '第一名',
  },
  2: {
    order: 'order-1 min-h-49 border-t-[#aeb5bf]',
    badge: 'border-[#b9bec6] bg-[#d9dde2] text-[#515861]',
    label: '第二名',
  },
  3: {
    order: 'order-3 min-h-47 border-t-[#bd7441]',
    badge: 'border-[#c78558] bg-[#dda075] text-[#713b1e]',
    label: '第三名',
  },
} as const;

function formatIq(value: number) {
  return new Intl.NumberFormat('zh-CN', {
    minimumFractionDigits: 1,
    maximumFractionDigits: 2,
  }).format(value);
}

export function IntelligencePodium({ entries }: { entries: IntelligenceRankingEntry[] }) {
  const podiumEntries = entries
    .filter((entry) => entry.rank >= 1 && entry.rank <= 3)
    .sort((left, right) => left.rank - right.rank);

  if (podiumEntries.length < 3) return null;

  return (
    <section aria-label="前三名" className="grid grid-cols-3 items-end gap-3">
      {podiumEntries.map((entry) => {
        const meta = podiumMeta[entry.rank as keyof typeof podiumMeta];
        return (
          <article
            className={`${meta.order} flex min-w-0 flex-col items-center justify-center rounded-md border border-t-4 border-rp-line bg-rp-raised px-4 py-4 text-center`}
            data-testid={`podium-rank-${entry.rank}`}
            key={`${entry.rank}-${entry.model}-${entry.effort}`}
          >
            <span
              aria-label={meta.label}
              className={`${meta.badge} grid size-12 flex-none place-items-center rounded-full border text-xl font-semibold [font-variant-numeric:tabular-nums]`}
            >
              {entry.rank}
            </span>
            <strong
              className="mt-3 block w-full truncate text-base leading-6 font-semibold"
              title={entry.model}
            >
              {entry.model}
            </strong>
            <span
              className="mt-2 inline-block max-w-full truncate rounded-sm bg-rp-subtle px-2 py-0.5 text-xs leading-5 text-rp-secondary"
              title={entry.effort}
            >
              {entry.effort}
            </span>
            <span className="mt-2 text-2xl leading-8 font-semibold [font-variant-numeric:tabular-nums]">
              {formatIq(entry.iq)}
            </span>
          </article>
        );
      })}
    </section>
  );
}
