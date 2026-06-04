// Renders the per-schedule (or aggregate) projected monthly cost in USD.
// Color coding follows the cost-cap convention: green ≤ $50, amber
// $50..$200, red > $200. Operators can tune those thresholds by editing
// the `tier` function below; the API only ships the raw projection.

export function CostProjectionBadge({
  usd,
  acknowledged = true,
}: {
  usd: number;
  acknowledged?: boolean;
}) {
  const tier = costTier(usd);
  const acknowledgedLabel = acknowledged ? "" : " · pending ack";
  return (
    <span
      data-testid="cost-projection-badge"
      data-cost-tier={tier.id}
      className={`inline-flex items-baseline gap-1 rounded px-2 py-0.5 text-xs font-medium ${tier.classes}`}
      aria-label={`Projected monthly cost ${formatUsd(usd)}${acknowledgedLabel}`}
      title={tier.label}
    >
      <span className="font-mono">{formatUsd(usd)}</span>
      <span className="text-[10px] uppercase tracking-wide opacity-80">
        /mo
      </span>
      {!acknowledged ? <span aria-hidden>!</span> : null}
    </span>
  );
}

function formatUsd(usd: number): string {
  if (Number.isNaN(usd)) return "—";
  if (usd >= 100) return `$${Math.round(usd)}`;
  if (usd >= 10) return `$${usd.toFixed(1)}`;
  return `$${usd.toFixed(2)}`;
}

type CostTier = {
  id: "low" | "medium" | "high";
  label: string;
  classes: string;
};

function costTier(usd: number): CostTier {
  if (usd <= 50) {
    return {
      id: "low",
      label: "Within the comfortable Phase 2 default cap",
      classes:
        "bg-emerald-100 dark:bg-emerald-900 text-emerald-800 dark:text-emerald-200",
    };
  }
  if (usd <= 200) {
    return {
      id: "medium",
      label: "Approaching the cost cap; ack required at declare time",
      classes:
        "bg-amber-100 dark:bg-amber-900 text-amber-900 dark:text-amber-200",
    };
  }
  return {
    id: "high",
    label: "Over the cost cap; ack required",
    classes:
      "bg-rose-100 dark:bg-rose-900 text-rose-900 dark:text-rose-200",
  };
}
