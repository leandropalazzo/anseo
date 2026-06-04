import { CompetitorTile } from "./competitor-tile";
import type { ShareRow } from "./stacked-area";

export interface HeadToHeadProps {
  /** Share-of-voice snapshot for all subjects (from `/comparisons`). */
  rows: ReadonlyArray<ShareRow>;
  /** First subject name to compare. */
  a: string;
  /** Second subject name to compare. */
  b: string;
}

/**
 * Head-to-head snapshot between two subjects. The `/comparisons` contract has
 * no time dimension, so this compares the two subjects' current-window share
 * of voice and reports the lead in percentage points.
 */
export function HeadToHead({ rows, a, b }: HeadToHeadProps) {
  const shareOf = (name: string) =>
    rows.find((r) => r.name.toLowerCase() === name.toLowerCase())?.share ?? 0;
  const shareA = shareOf(a);
  const shareB = shareOf(b);
  const leadPp = (shareA - shareB) * 100;

  return (
    <div>
      <div className="grid grid-cols-2 gap-[14px]">
        <CompetitorTile name={a} share={shareA} accent />
        <CompetitorTile name={b} share={shareB} />
      </div>
      <div className="mt-[12px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] p-[10px]">
        <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
          share-of-voice gap
        </div>
        <div className="mt-[4px] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
          {a}{" "}
          {leadPp >= 0 ? "leads" : "trails"} {b} by{" "}
          <strong
            className="font-[family-name:var(--font-mono)]"
            style={{ color: leadPp >= 0 ? "var(--ok)" : "var(--danger)" }}
          >
            {leadPp >= 0 ? "+" : ""}
            {leadPp.toFixed(1)}pp
          </strong>{" "}
          in the current window.
        </div>
      </div>
    </div>
  );
}
