import Link from "next/link";

import type { RecommendationTraceability } from "@/lib/api";

// UX-DR105 — every recommendation's traceability is surfaced as keyboard-
// reachable links back to the underlying Prompt Runs and Citations. Each chip
// is a real <a> (via next/link) so Tab/Enter traversal works without JS.
// UX-DR103 — an empty traceability block is a render error, not a blank panel;
// the engine guarantees at least one source, so a missing one signals a bug.

function chipClass(): string {
  return [
    "inline-flex items-center gap-[4px]",
    "border border-[color:var(--border)]",
    "px-[6px] py-[2px]",
    "font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]",
    "text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
    "focus-visible:outline focus-visible:outline-1 focus-visible:outline-[color:var(--accent)]",
  ].join(" ");
}

function shortId(id: string): string {
  return id.length > 10 ? `${id.slice(0, 8)}…` : id;
}

export function EvidenceChips({
  traceability,
}: {
  traceability: RecommendationTraceability;
}) {
  const runs = traceability.source_run_ids ?? [];
  const citations = traceability.source_citation_ids ?? [];
  const empty = runs.length === 0 && citations.length === 0;

  if (empty) {
    return (
      <div
        data-testid="rec-evidence-error"
        role="alert"
        className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--danger)]"
      >
        evidence unavailable — recommendation has no traceable sources
      </div>
    );
  }

  return (
    <div
      data-testid="rec-evidence"
      className="flex flex-col gap-[8px]"
    >
      {runs.length > 0 && (
        <div className="flex flex-wrap items-center gap-[6px]">
          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            prompt runs
          </span>
          {runs.map((id) => (
            <Link
              key={id}
              href={`/runs/${encodeURIComponent(id)}`}
              data-testid="rec-evidence-run"
              className={chipClass()}
            >
              run:{shortId(id)}
            </Link>
          ))}
          {traceability.source_run_ids_truncated && (
            <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              +more
            </span>
          )}
        </div>
      )}
      {citations.length > 0 && (
        <div className="flex flex-wrap items-center gap-[6px]">
          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            citations
          </span>
          {citations.map((id) => (
            <Link
              key={id}
              href={`/citations#${encodeURIComponent(id)}`}
              data-testid="rec-evidence-citation"
              className={chipClass()}
            >
              cite:{shortId(id)}
            </Link>
          ))}
          {traceability.source_citation_ids_truncated && (
            <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
              +more
            </span>
          )}
        </div>
      )}
    </div>
  );
}
