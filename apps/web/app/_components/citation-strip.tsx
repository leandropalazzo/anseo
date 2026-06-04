import { Bar } from "@/components/charts/bar";

/** Slim citation row for the Overview strip (derived from the live
 *  `CitationSummaryRow` from `lib/api/citations.ts`). */
export interface CitationStripRow {
  domain: string;
  frequency: number;
}

export interface CitationStripProps {
  /** Live citation summary rows (from `fetchCitationSummary`). */
  rows: ReadonlyArray<CitationStripRow>;
}

export function CitationStrip({ rows }: CitationStripProps) {
  const top = rows.slice(0, 6);
  if (top.length === 0) return null;
  const max = top[0].frequency || 1;
  return (
    <div className="flex flex-col gap-[8px]">
      {top.map((c) => (
        <div
          key={c.domain}
          className="grid grid-cols-[180px_1fr_60px] items-center gap-[12px]"
        >
          <span className="overflow-hidden text-ellipsis whitespace-nowrap font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
            {c.domain}
          </span>
          <Bar value={c.frequency} max={max} color="var(--accent)" height={4} />
          <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
            {c.frequency}
          </span>
        </div>
      ))}
    </div>
  );
}
