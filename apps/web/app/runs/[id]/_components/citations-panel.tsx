import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import type { RunCitationEntry } from "@/lib/api/run-detail";

export interface CitationsPanelProps {
  /** Live citations for this run (single (run,provider) pair). */
  citations: ReadonlyArray<RunCitationEntry>;
  /** True when `citations` are demo data shown under `OGEO_DEMO=1`. */
  isDemo?: boolean;
}

export function CitationsPanel({ citations, isDemo = false }: CitationsPanelProps) {
  if (citations.length === 0) {
    return (
      <EmptyState
        title="No citations extracted"
        message="No source citations were extracted from this run's response."
      />
    );
  }

  return (
    <Card
      eyebrow={`extracted from this run · ${citations.length} citations`}
      title="Citations"
      action={isDemo ? <DemoBadge /> : undefined}
    >
      <div className="flex flex-col">
        {citations.map((c, i) => {
          return (
            <div
              key={c.id}
              className="grid grid-cols-[1fr_90px_220px] items-center gap-[10px] py-[8px]"
              style={{
                borderBottom:
                  i === citations.length - 1
                    ? undefined
                    : "1px solid var(--hairline)",
              }}
            >
              <div>
                <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                  {c.url ?? c.domain}
                </div>
                <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {c.domain}
                </div>
              </div>
              <Pill mono>{c.source_type ?? "unknown"}</Pill>
              <div className="flex items-center gap-[6px]">
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  cited by:
                </span>
                <ProviderDot provider={c.provider} />
                <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  freq {c.frequency}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}
