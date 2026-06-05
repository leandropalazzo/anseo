import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { Icon, ICON_DEFAULTS } from "@/lib/icons";
import type { RunProvenanceStep } from "@/lib/api/run-detail";

export interface ProvenancePanelProps {
  /** Live provenance steps. The Phase 1 schema has no provenance model, so
   *  this is always empty today (the endpoint returns `[]`). */
  steps: ReadonlyArray<RunProvenanceStep>;
}

export function ProvenancePanel({ steps }: ProvenancePanelProps) {
  if (steps.length === 0) {
    return (
      <EmptyState
        title="No provenance recorded"
        message="Lifecycle / provenance steps are not yet captured for runs."
      />
    );
  }

  return (
    <Card eyebrow="every metric drills to raw" title="Provenance trail">
      <div className="flex flex-col">
        {steps.map((s, i) => (
          <div
            key={i}
            className="grid grid-cols-[180px_28px_1fr] items-center gap-[12px] py-[6px]"
            style={{
              borderBottom:
                i === steps.length - 1
                  ? undefined
                  : "1px solid var(--hairline)",
            }}
          >
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              {s.at}
            </span>
            <span className="inline-flex h-[22px] w-[22px] items-center justify-center border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] text-[color:var(--text-muted)]">
              <Icon.Activity size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            </span>
            <div>
              <div className="text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                {s.step}
              </div>
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                {s.status}
              </div>
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}
