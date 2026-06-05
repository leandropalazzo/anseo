import Link from "next/link";

import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import { Bar } from "@/components/charts/bar";
import { fetchSentiment, type SentimentPoint } from "@/lib/api";

interface SearchParams {
  days?: string;
}

function pickDays(raw: string | undefined): 7 | 30 | 90 {
  const n = Number(raw);
  if (n === 7 || n === 30 || n === 90) return n;
  return 30;
}

interface EntityRoll {
  entity: string;
  positive: number;
  neutral: number;
  negative: number;
  total: number;
  scoreSum: number;
}

/** Collapse per-(prompt,provider,entity,day) points into per-entity rollups. */
function rollByEntity(points: SentimentPoint[]): EntityRoll[] {
  const map = new Map<string, EntityRoll>();
  for (const p of points) {
    const r =
      map.get(p.entity) ??
      { entity: p.entity, positive: 0, neutral: 0, negative: 0, total: 0, scoreSum: 0 };
    r.positive += p.positive;
    r.neutral += p.neutral;
    r.negative += p.negative;
    r.total += p.total;
    r.scoreSum += p.average_score * p.total;
    map.set(p.entity, r);
  }
  return [...map.values()].sort((a, b) => b.total - a.total);
}

const WINDOWS: ReadonlyArray<7 | 30 | 90> = [7, 30, 90];

export default async function SentimentPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const sp = await searchParams;
  const days = pickDays(sp.days);

  let points: SentimentPoint[] = [];
  try {
    const r = await fetchSentiment(days);
    points = r.points;
  } catch {
    points = [];
  }
  const rolls = rollByEntity(points);

  return (
    <section data-testid="sentiment-page" className="space-y-[12px]">
      <PageHeader
        title="Sentiment"
        description="Tone of every classified brand mention — positive, neutral, or negative — across providers."
        actions={
          <div className="flex items-center border border-[color:var(--border)]">
            {WINDOWS.map((w) => (
              <Link
                key={w}
                href={`/sentiment?days=${w}`}
                aria-current={w === days ? "page" : undefined}
                className={[
                  "px-[10px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]",
                  w === days
                    ? "bg-[color:var(--bg-elev-2)] text-[color:var(--text)]"
                    : "text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
                ].join(" ")}
              >
                {w}d
              </Link>
            ))}
          </div>
        }
      />

      {rolls.length === 0 ? (
        <EmptyState
          title="No classified mentions yet"
          hint="Run prompts so the extractor can classify mention tone. CLI parity: ogeo report --since 30d"
        />
      ) : (
        <div className="grid grid-cols-1 gap-[12px] md:grid-cols-2">
          {rolls.map((r) => {
            const avg = r.total > 0 ? r.scoreSum / r.total : 0;
            const share = (n: number) => (r.total > 0 ? n / r.total : 0);
            return (
              <Card key={r.entity} eyebrow="entity" title={r.entity} accent>
                <div className="flex items-baseline justify-between">
                  <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    {r.total} mention{r.total === 1 ? "" : "s"}
                  </div>
                  <div className="text-[length:18px] text-[color:var(--text)]">
                    {avg.toFixed(0)}
                    <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                      /100
                    </span>
                  </div>
                </div>
                <div className="mt-[12px] space-y-[8px]">
                  <SentimentRow label="positive" value={share(r.positive)} count={r.positive} color="var(--ok)" />
                  <SentimentRow label="neutral" value={share(r.neutral)} count={r.neutral} color="var(--text-faint)" />
                  <SentimentRow label="negative" value={share(r.negative)} count={r.negative} color="var(--danger)" />
                </div>
              </Card>
            );
          })}
        </div>
      )}
    </section>
  );
}

function SentimentRow({
  label,
  value,
  count,
  color,
}: {
  label: string;
  value: number;
  count: number;
  color: string;
}) {
  return (
    <div className="flex items-center gap-[10px]">
      <div className="w-[64px] shrink-0 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase text-[color:var(--text-muted)]">
        {label}
      </div>
      <Bar value={value} max={1} color={color} ariaLabel={`${label} share`} className="flex-1" />
      <div className="w-[64px] shrink-0 text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        {(value * 100).toFixed(0)}% · {count}
      </div>
    </div>
  );
}
