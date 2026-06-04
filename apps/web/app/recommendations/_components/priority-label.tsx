import { Pill, type PillTone } from "@/components/ui/pill";
import type {
  Recommendation,
  RecommendationSeverity,
} from "@/lib/api";
import { isNonDeterministic } from "@/lib/api";

// UX-DR107 — priority is rendered as a mono token plus a human label so the
// operator can scan severity without decoding a color alone.
const SEVERITY_TONE: Record<RecommendationSeverity, PillTone> = {
  high: "danger",
  medium: "warn",
  low: "info",
  info: "neutral",
};

const SEVERITY_LABEL: Record<RecommendationSeverity, string> = {
  high: "High",
  medium: "Medium",
  low: "Low",
  info: "Info",
};

export function PriorityLabel({
  severity,
}: {
  severity: RecommendationSeverity;
}) {
  return (
    <span
      data-testid="rec-priority"
      data-severity={severity}
      className="inline-flex items-center gap-[6px]"
    >
      <Pill mono tone={SEVERITY_TONE[severity]}>
        {severity}
      </Pill>
      <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
        {SEVERITY_LABEL[severity]}
      </span>
    </span>
  );
}

// UX-DR106 / OQ-P3-20 — every recommendation produced by a non-deterministic
// pipeline carries this marker. Callers also suppress hard-outcome copy
// (UX-DR109) when this is shown.
export function NdpMarker() {
  return (
    <span
      data-testid="rec-ndp-marker"
      title="Non-deterministic pipeline — results may vary between runs"
      className="inline-flex items-center font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--warn)]"
    >
      ⚠ NDP
    </span>
  );
}

/** Renders the NDP marker only when the rec is non-deterministic. */
export function NdpMarkerFor({ rec }: { rec: Recommendation }) {
  if (!isNonDeterministic(rec)) return null;
  return <NdpMarker />;
}
