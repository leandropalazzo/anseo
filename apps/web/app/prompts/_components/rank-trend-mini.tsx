import { Sparkline } from "@/components/charts/sparkline";
import { ProviderDot } from "@/components/ui/provider-dot";
import { genTrend, PROVIDERS, type MockPrompt } from "@/lib/mock";
import { PROVIDER_COLORS } from "@/lib/provider-colors";

export interface RankTrendMiniProps {
  prompt: MockPrompt;
}

/** Compact 4-row sparkline list, one per provider, for last-14-day rank. */
export function RankTrendMini({ prompt }: RankTrendMiniProps) {
  const trends = PROVIDERS.map((p) => ({
    p,
    points: genTrend("healthy", p, prompt, 14).map((x) => -x.avg_rank),
    last: genTrend("healthy", p, prompt, 14).at(-1)?.avg_rank ?? 0,
  }));

  return (
    <div className="flex flex-col gap-[8px]">
      {trends.map((t) => (
        <div
          key={t.p}
          className="grid grid-cols-[100px_1fr_40px] items-center gap-[10px]"
        >
          <span className="flex items-center gap-[6px]">
            <ProviderDot provider={t.p} />
            <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              {PROVIDER_COLORS[t.p].label}
            </span>
          </span>
          <Sparkline
            points={t.points}
            color={`var(${PROVIDER_COLORS[t.p].var})`}
            width={220}
            height={20}
            stroke={1.2}
            ariaLabel={`${PROVIDER_COLORS[t.p].label} rank trend`}
          />
          <span className="text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
            {t.last.toFixed(1)}
          </span>
        </div>
      ))}
    </div>
  );
}
