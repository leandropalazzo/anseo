import { Pill } from "@/components/ui/pill";

/**
 * One row of the Movers list, derived in `page.tsx` from the live
 * share-of-voice snapshot (`/comparisons`) plus the `/brands` roster.
 *
 * The `/comparisons` contract carries no historical delta, so this surfaces a
 * current-window `share` and the `avg_rank_7d` from `/brands` instead of a
 * day-over-day ±pp change. Tracked brands absent from the comparison matrix are
 * flagged via `isNew`.
 */
export interface MoverRow {
  name: string;
  /** Fraction in [0,1] of total window mentions. */
  share: number;
  /** 7-day average ranking from `/brands`, or null when never ranked. */
  avgRank: number | null;
  /** True for brands tracked but not yet present in the comparison matrix. */
  isNew?: boolean;
}

export interface MoversProps {
  rows: ReadonlyArray<MoverRow>;
}

/**
 * Movers list — competitors ordered by share-of-voice, surfacing each one's
 * current window share and average ranking, with new entrants flagged.
 */
export function Movers({ rows }: MoversProps) {
  return (
    <div className="flex flex-col gap-[6px]">
      {rows.map((m) => (
        <div
          key={m.name}
          className="grid items-center gap-[10px] border border-[color:var(--hairline)] bg-[color:var(--bg-elev-2)] px-[10px] py-[8px]"
          style={{ gridTemplateColumns: "1fr 70px 70px" }}
        >
          <span
            className="inline-flex items-center gap-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)]"
            style={{ color: m.isNew ? "var(--warn)" : "var(--text)" }}
          >
            {m.name}
            {m.isNew && <Pill tone="warn">NEW</Pill>}
          </span>
          <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
            {(m.share * 100).toFixed(1)}%
          </span>
          <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            {m.avgRank === null ? "—" : `#${m.avgRank.toFixed(1)}`}
          </span>
        </div>
      ))}
    </div>
  );
}
