import { Card } from "@/components/ui/card";
import type { CitationScore } from "@/lib/api";

/** Color band for the headline number, by score tier. */
function toneForScore(score: number): "ok" | "info" | "warn" | "danger" {
  if (score >= 75) return "ok";
  if (score >= 50) return "info";
  if (score >= 25) return "warn";
  return "danger";
}

interface ComponentBarProps {
  label: string;
  /** Earned points for this component. */
  value: number;
  /** Max points this component can contribute. */
  max: number;
  hint: string;
}

function ComponentBar({ label, value, max, hint }: ComponentBarProps) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    <div className="flex flex-col gap-[3px]" title={hint}>
      <div className="flex items-baseline justify-between font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
        <span className="text-[color:var(--text-muted)]">{label}</span>
        <span className="text-[color:var(--text-faint)]">
          {value.toFixed(1)} / {max}
        </span>
      </div>
      <div className="h-[6px] w-full bg-[color:var(--bg-sunken)]">
        <div
          className="h-full"
          style={{
            width: `${pct}%`,
            background: "color-mix(in oklch, var(--info) 70%, transparent)",
          }}
        />
      </div>
    </div>
  );
}

export function CitationScoreCard({
  score,
  windowDays,
}: {
  score: CitationScore;
  windowDays: number;
}) {
  const tone = toneForScore(score.score);
  const growthPct =
    score.growth_rate === null ? null : Math.round(score.growth_rate * 100);

  return (
    <Card eyebrow={`citation score · last ${windowDays}d`} title="Footprint health">
      <div className="grid gap-[16px] sm:grid-cols-[auto_1fr] sm:items-center">
        <div className="flex flex-col items-center justify-center px-[8px]">
          <div
            className="font-[family-name:var(--font-mono)] text-[length:48px] leading-none"
            style={{ color: `color-mix(in oklch, var(--${tone}) 85%, var(--text))` }}
          >
            {score.score.toFixed(0)}
          </div>
          <div className="mt-[2px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            / 100
          </div>
          {growthPct !== null && (
            <div
              className="mt-[6px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
              style={{
                color:
                  growthPct >= 0
                    ? "color-mix(in oklch, var(--ok) 80%, var(--text))"
                    : "color-mix(in oklch, var(--danger) 80%, var(--text))",
              }}
            >
              {growthPct >= 0 ? "▲" : "▼"} {Math.abs(growthPct)}% vs prior
            </div>
          )}
        </div>

        <div className="flex flex-col gap-[10px]">
          <div className="grid grid-cols-3 gap-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]">
            <div className="flex flex-col">
              <span className="text-[color:var(--text)] text-[length:var(--font-size-sm)]">
                {score.total_citations}
              </span>
              <span className="text-[color:var(--text-faint)]">citations</span>
            </div>
            <div className="flex flex-col">
              <span className="text-[color:var(--text)] text-[length:var(--font-size-sm)]">
                {score.distinct_domains}
              </span>
              <span className="text-[color:var(--text-faint)]">domains</span>
            </div>
            <div className="flex flex-col">
              <span className="text-[color:var(--text)] text-[length:var(--font-size-sm)]">
                {Math.round(score.quality_share * 100)}%
              </span>
              <span className="text-[color:var(--text-faint)]">authoritative</span>
            </div>
          </div>
          <div className="flex flex-col gap-[8px]">
            <ComponentBar
              label="Volume"
              value={score.volume_component}
              max={40}
              hint="Total citations, saturating at 100."
            />
            <ComponentBar
              label="Diversity"
              value={score.diversity_component}
              max={30}
              hint="Distinct cited domains, saturating at 30."
            />
            <ComponentBar
              label="Quality"
              value={score.quality_component}
              max={30}
              hint="Share of citations from authoritative web sources vs UGC/social."
            />
          </div>
        </div>
      </div>
    </Card>
  );
}
