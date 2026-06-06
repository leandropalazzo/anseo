import { chartColor } from "@/lib/chart-colors";

/** One subject's share of the comparison-window mention total. */
export interface ShareRow {
  name: string;
  /** Fraction in [0,1] of total mentions across the window. */
  share: number;
}

export interface ShareOfVoiceChartProps {
  rows: ReadonlyArray<ShareRow>;
  /** Primary brand name (highlighted band), case-insensitive. */
  primary?: string;
}

/**
 * Share-of-voice snapshot. The /comparisons contract carries no daily series,
 * so this renders a single stacked horizontal bar of the current-window share
 * per subject, each band labelled so non-color readers can identify it.
 *
 * Series colors come from the shared `CHART_RAMP` (chart-color.ts) — band 0
 * is `--chart-1` (== `--accent`), so the primary brand keeps its yellow band.
 */
export function ShareOfVoiceChart({ rows, primary = "" }: ShareOfVoiceChartProps) {
  if (rows.length === 0) return null;
  const total = rows.reduce((acc, r) => acc + r.share, 0) || 1;
  const colorFor = (i: number) => chartColor(i);

  const bands = rows.map((r, i) => {
    const before = rows
      .slice(0, i)
      .reduce((sum, prev) => sum + prev.share, 0);
    const lo = before / total;
    const hi = (before + r.share) / total;
    return { ...r, lo, hi, color: colorFor(i) };
  });

  return (
    <div className="space-y-[12px]">
      <div
        role="img"
        aria-label="Share of voice by competitor"
        className="flex h-[40px] w-full overflow-hidden border border-[color:var(--hairline)]"
      >
        {bands.map((b) => {
          const isPrimary = b.name.toLowerCase() === primary.toLowerCase();
          return (
            <div
              key={b.name}
              title={`${b.name} ${(b.share * 100).toFixed(1)}%`}
              style={{
                width: `${(b.hi - b.lo) * 100}%`,
                background: b.color,
                opacity: isPrimary ? 0.95 : 0.7,
              }}
            />
          );
        })}
      </div>
      <div className="grid gap-[6px] sm:grid-cols-2 lg:grid-cols-3">
        {bands.map((b) => {
          const isPrimary = b.name.toLowerCase() === primary.toLowerCase();
          return (
            <div
              key={b.name}
              className="flex items-center gap-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
            >
              <span
                aria-hidden
                className="inline-block h-[10px] w-[10px] shrink-0"
                style={{ background: b.color }}
              />
              <span
                className="text-[color:var(--text)]"
                style={{ fontWeight: isPrimary ? 600 : 400 }}
              >
                {b.name}
              </span>
              <span className="ml-auto text-[color:var(--text-faint)]">
                {(b.share * 100).toFixed(1)}%
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
