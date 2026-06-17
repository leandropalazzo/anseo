export interface HeroStripProps {
  brandName: string;
  avgRank?: number;
  /** Fraction 0–1: share of runs where brand was mentioned. */
  presenceRate?: number;
  mentions: number;
  successRate: number;
  totalRuns: number;
  failedCount: number;
  providerCount: number;
}

/**
 * Hero strip summarizing the primary brand's last-7d standing. All copy is
 * derived from live run-summary + brand data — no fabricated narrative.
 *
 * Presence rate (% of runs where brand appeared) is the primary metric.
 * Avg rank is secondary — it is conditional on the brand appearing at all
 * and overstates standing when presence rate is low.
 */
export function HeroStrip({
  brandName,
  avgRank,
  presenceRate,
  mentions,
  successRate,
  totalRuns,
  failedCount,
  providerCount,
}: HeroStripProps) {
  const tone: "ok" | "warn" = failedCount > 0 ? "warn" : "ok";

  const noRuns = totalRuns === 0;
  const label = noRuns
    ? "No runs · last 7d"
    : failedCount > 0
    ? `${failedCount} failed runs · last 7d`
    : "Healthy · all runs ok";

  const providerLabel = `${providerCount} provider${providerCount === 1 ? "" : "s"}`;

  const presencePct =
    presenceRate !== undefined ? Math.round(presenceRate * 100) : undefined;

  const headline = noRuns
    ? `No runs recorded for ${brandName} yet.`
    : presencePct !== undefined
    ? `${brandName} appeared in ${presencePct}% of measured runs across ${providerLabel}.`
    : mentions > 0
    ? `${brandName} was mentioned ${mentions} time${mentions === 1 ? "" : "s"} across ${providerLabel}.`
    : `${brandName} ran ${totalRuns} time${totalRuns === 1 ? "" : "s"} across ${providerLabel}, with no mentions yet.`;

  const rankNote =
    avgRank !== undefined
      ? `avg rank ${avgRank.toFixed(2)} when mentioned`
      : "no rank data";

  const sub = noRuns
    ? "Run a prompt (anseo prompt run) to populate the last-7-day window."
    : `${mentions} mention${mentions === 1 ? "" : "s"} · ${rankNote} · ${successRate.toFixed(0)}% run success · ${failedCount} failed. Presence rate is measured against your configured prompt set only.`;

  return (
    <div className="relative grid grid-cols-[1fr_auto] items-center gap-4 overflow-hidden border border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[18px]">
      <div
        aria-hidden
        className="absolute inset-y-0 left-0 w-[3px]"
        style={{ background: `var(--${tone})` }}
      />
      <div>
        <div
          className="label-eyebrow inline-flex items-center gap-[6px]"
          style={{ color: `var(--${tone})` }}
        >
          <span
            aria-hidden
            className="inline-block h-[6px] w-[6px] rounded-full"
            style={{ background: `var(--${tone})` }}
          />
          {label}
        </div>
        <h2 className="m-0 mt-[8px] text-balance text-[30px] font-normal leading-tight tracking-[var(--display-tracking)] text-[color:var(--text)]">
          {headline}
        </h2>
        <p className="m-0 mt-[4px] max-w-[720px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          {sub}
        </p>
      </div>
    </div>
  );
}
