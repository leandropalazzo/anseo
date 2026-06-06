import type { FunnelStep } from "@/lib/api";
import { chartColor } from "@/lib/chart-colors";

/**
 * Horizontal funnel: one bar per step, width proportional to the step's count
 * relative to the funnel's first (largest expected) step. Each bar is labelled
 * with its count and the drop-off from the previous step.
 *
 * Drop-off rendering follows the API contract (Story 47.4): `drop_off_pct` is
 * `null` for the first step and for the "tracking deployed mid-funnel" anomaly
 * where a later step has MORE events than the prior one — both render as
 * "N/A" / "—", never a negative percentage.
 *
 * All labels are React text nodes (auto-escaped) — no `dangerouslySetInnerHTML`,
 * so a malicious step label can't inject markup (guards the category-injection
 * class of bug flagged in a prior analytics story).
 */
export function FunnelChart({ steps }: { steps: FunnelStep[] }) {
  const max = Math.max(1, ...steps.map((s) => s.count));
  return (
    <div className="flex flex-col gap-[10px]" data-testid="funnel-chart">
      {steps.map((step, i) => {
        const widthPct = Math.max(2, (step.count / max) * 100);
        const dropOff =
          step.drop_off_pct === null
            ? i === 0
              ? "—"
              : "N/A"
            : `−${step.drop_off_pct.toFixed(1)}%`;
        return (
          <div key={step.label} className="flex flex-col gap-[3px]">
            <div className="flex items-center justify-between font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              <span className="truncate">{step.label}</span>
              <span className="shrink-0 tabular-nums">
                {step.count.toLocaleString()}
                {i > 0 && (
                  <span
                    className="ml-[8px] text-[color:var(--text-faint)]"
                    data-testid={`funnel-dropoff-${i}`}
                    title="drop-off from previous step"
                  >
                    {dropOff}
                  </span>
                )}
              </span>
            </div>
            <div className="h-[14px] w-full bg-[color:var(--bg-sunken)]">
              <div
                className="h-full"
                style={{
                  width: `${widthPct}%`,
                  backgroundColor: chartColor(i),
                }}
                aria-label={`${step.label}: ${step.count}`}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
