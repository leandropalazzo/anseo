import { Bar } from "@/components/charts/bar";
import { Card } from "@/components/ui/card";
import type { CitationSummaryRow } from "@/lib/api";

export interface DomainsByTypeProps {
  /** Live citation-summary rows; grouped here by `source_type`. */
  rows: ReadonlyArray<CitationSummaryRow>;
}

export function DomainsByType({ rows }: DomainsByTypeProps) {
  const groups = new Map<string, CitationSummaryRow[]>();
  for (const c of rows) {
    const source = c.source_type ?? "unknown";
    const arr = groups.get(source) ?? [];
    arr.push(c);
    groups.set(source, arr);
  }
  const entries = Array.from(groups.entries()).sort(
    ([, a], [, b]) =>
      b.reduce((acc, x) => acc + x.frequency, 0) -
      a.reduce((acc, x) => acc + x.frequency, 0),
  );
  const max = Math.max(1, ...rows.map((r) => r.frequency));

  return (
    <div className="grid gap-[12px] sm:grid-cols-2 lg:grid-cols-3">
      {entries.map(([source, group]) => (
        <Card key={source} eyebrow={source} title={`${group.length} domains`} accent>
          {group.map((c) => (
            <div
              key={c.domain}
              className="grid items-center gap-[8px] py-[4px]"
              style={{ gridTemplateColumns: "1fr 60px 30px" }}
            >
              <span className="truncate font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text)]">
                {c.domain}
              </span>
              <Bar
                value={c.frequency}
                max={max}
                color="var(--accent)"
                height={4}
                ariaLabel={`${c.domain} frequency`}
              />
              <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                {c.frequency}
              </span>
            </div>
          ))}
        </Card>
      ))}
    </div>
  );
}
