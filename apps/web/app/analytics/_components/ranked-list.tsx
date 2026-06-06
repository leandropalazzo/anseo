import type { ReactNode } from "react";

import { EmptyState } from "@/components/ui/empty-state";

export interface RankedRow {
  /** Display label — a site path or a referrer domain. Rendered as React text
   *  (auto-escaped); never interpolated into markup. */
  label: string;
  value: number;
}

/**
 * Top-N ranked table with a proportional inline bar. Used for top pages and top
 * referrer domains. Renders its own empty state so a missing dimension doesn't
 * leave a blank card.
 */
export function RankedList({
  rows,
  unitLabel,
  emptyTitle,
  emptyHint,
}: {
  rows: RankedRow[];
  unitLabel: ReactNode;
  emptyTitle: ReactNode;
  emptyHint: ReactNode;
}) {
  if (rows.length === 0) {
    return <EmptyState title={emptyTitle} hint={emptyHint} />;
  }
  const max = Math.max(1, ...rows.map((r) => r.value));
  return (
    <ul className="flex flex-col gap-[8px]" data-testid="ranked-list">
      {rows.map((r) => (
        <li key={r.label} className="flex flex-col gap-[3px]">
          <div className="flex items-center justify-between gap-[8px] text-[length:var(--font-size-sm)]">
            <span
              className="min-w-0 truncate font-[family-name:var(--font-mono)] text-[color:var(--text)]"
              title={r.label}
            >
              {r.label}
            </span>
            <span className="shrink-0 tabular-nums text-[color:var(--text-muted)]">
              {r.value.toLocaleString()}{" "}
              <span className="text-[color:var(--text-faint)]">{unitLabel}</span>
            </span>
          </div>
          <div className="h-[4px] w-full bg-[color:var(--bg-sunken)]">
            <div
              className="h-full bg-[color:var(--accent)]"
              style={{ width: `${(r.value / max) * 100}%` }}
              aria-hidden
            />
          </div>
        </li>
      ))}
    </ul>
  );
}
