export interface CompetitorTileProps {
  name: string;
  /** Current-window fractional share in [0,1]. */
  share: number;
  accent?: boolean;
}

/**
 * A single competitor tile showing the subject's current-window share of
 * voice. The `/comparisons` contract is a snapshot (no daily series), so this
 * renders the headline figure only.
 */
export function CompetitorTile({ name, share, accent = false }: CompetitorTileProps) {
  return (
    <div className="border border-[color:var(--border)] p-[10px]">
      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        {name}
      </div>
      <div
        className="font-[family-name:var(--font-display)] text-[28px] leading-tight tracking-[var(--display-tracking)]"
        style={{ color: accent ? "var(--accent)" : "var(--text)" }}
      >
        {(share * 100).toFixed(1)}%
      </div>
    </div>
  );
}
