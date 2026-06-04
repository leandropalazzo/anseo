import { Bar } from "@/components/charts/bar";

export interface CompetitorBarRow {
  name: string;
  /** 7d mention count used to derive share of voice. */
  mentions: number;
  /** Whether this is the operator's primary brand. */
  isPrimary: boolean;
}

export interface CompetitorBarsProps {
  /** Brand roster with 7d mention counts (from `fetchBrands`). */
  rows: ReadonlyArray<CompetitorBarRow>;
}

export function CompetitorBars({ rows }: CompetitorBarsProps) {
  const total = rows.reduce((a, r) => a + r.mentions, 0);
  const ranked = [...rows].sort((a, b) => b.mentions - a.mentions);
  return (
    <div className="flex flex-col gap-[8px]">
      {ranked.map((c) => {
        const share = total > 0 ? c.mentions / total : 0;
        return (
          <div
            key={c.name}
            className="grid grid-cols-[120px_1fr_60px] items-center gap-[12px]"
          >
            <span
              className="inline-flex items-center gap-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)]"
              style={{
                color: c.isPrimary ? "var(--text)" : "var(--text-muted)",
                fontWeight: c.isPrimary ? 600 : 400,
              }}
            >
              {c.isPrimary && (
                <span
                  aria-hidden
                  className="inline-block h-[5px] w-[5px] rounded-full"
                  style={{ background: "var(--accent)" }}
                />
              )}
              {c.name}
            </span>
            <Bar
              value={share}
              max={1}
              color={c.isPrimary ? "var(--accent)" : "var(--text-faint)"}
              height={6}
              ariaLabel={`${c.name} share of voice`}
            />
            <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              {(share * 100).toFixed(1)}%
            </span>
          </div>
        );
      })}
    </div>
  );
}
