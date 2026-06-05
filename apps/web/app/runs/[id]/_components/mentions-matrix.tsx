import { DemoBadge } from "@/components/demo-badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Card } from "@/components/ui/card";
import { ProviderDot } from "@/components/ui/provider-dot";
import type { RunMentionEntry } from "@/lib/api/run-detail";
import { resolveProviderIdentity } from "@/lib/provider-colors";

export interface MentionsMatrixProps {
  /** Live mentions for this run. A run is a single (run,provider) pair, so
   *  every row shares the same `provider`. */
  mentions: ReadonlyArray<RunMentionEntry>;
  /** True when `mentions` are demo data shown under `OGEO_DEMO=1`. */
  isDemo?: boolean;
}

function rankCellStyle(r: number): React.CSSProperties {
  if (r <= 3) {
    return {
      background: "color-mix(in oklch, var(--ok) 14%, transparent)",
      color: "var(--ok)",
    };
  }
  if (r <= 5) {
    return {
      background: "color-mix(in oklch, var(--warn) 14%, transparent)",
      color: "var(--warn)",
    };
  }
  return {
    background: "color-mix(in oklch, var(--danger) 14%, transparent)",
    color: "var(--danger)",
  };
}

export function MentionsMatrix({ mentions, isDemo = false }: MentionsMatrixProps) {
  if (mentions.length === 0) {
    return (
      <EmptyState
        title="No mentions extracted"
        message="No brand or competitor mentions were extracted from this run's response."
      />
    );
  }

  // The run carries a single provider; derive it from the rows.
  const provider = mentions[0]!.provider;
  const providerLabel = resolveProviderIdentity(provider).label;

  // Sort by rank (best first), stable on entity for ties.
  const rows = [...mentions].sort(
    (a, b) => a.rank - b.rank || a.entity.localeCompare(b.entity),
  );

  return (
    <Card
      eyebrow={`rank matrix · ${providerLabel}`}
      title="Mentions"
      action={isDemo ? <DemoBadge /> : undefined}
    >
      <div className="overflow-auto">
        <table className="w-full border-collapse">
          <thead>
            <tr>
              <th className="label-eyebrow border-b border-[color:var(--border)] px-[12px] py-[6px] text-left text-[color:var(--text-faint)]">
                entity
              </th>
              <th className="label-eyebrow border-b border-[color:var(--border)] px-[12px] py-[6px] text-left text-[color:var(--text-faint)]">
                <span className="inline-flex items-center gap-[6px]">
                  <ProviderDot provider={provider} />
                  <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
                    {providerLabel}
                  </span>
                </span>
              </th>
              <th className="label-eyebrow border-b border-[color:var(--border)] px-[12px] py-[6px] text-left text-[color:var(--text-faint)]">
                matched text
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((m) => {
              const ours = m.entity.toLowerCase() === "pinecone";
              return (
                <tr
                  key={m.id}
                  className="border-b border-[color:var(--hairline)]"
                >
                  <td
                    className="px-[12px] py-[6px] text-[length:var(--font-size-sm)]"
                    style={{
                      color: ours ? "var(--text)" : "var(--text-muted)",
                      fontWeight: ours ? 600 : 400,
                    }}
                  >
                    {ours && (
                      <span
                        aria-hidden
                        className="mr-[6px] inline-block h-[5px] w-[5px] rounded-full"
                        style={{ background: "var(--accent)" }}
                      />
                    )}
                    {m.entity}
                  </td>
                  <td className="px-[12px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)]">
                    <span className="px-[6px] py-[1px]" style={rankCellStyle(m.rank)}>
                      {m.rank}
                    </span>
                  </td>
                  <td className="px-[12px] py-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    {m.matched_text}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}
