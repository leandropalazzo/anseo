import { Bar } from "@/components/charts/bar";
import { ProviderDot } from "@/components/ui/provider-dot";
import { resolveProviderIdentity } from "@/lib/provider-colors";

export interface ProviderRankRow {
  provider: string;
  count: number;
  rank: number;
  rate: number;
}

export interface ProviderRanksProps {
  providers: ReadonlyArray<ProviderRankRow>;
  /** Primary brand name (lower-cased for the footer caption). */
  brand: string;
  /** Distinct prompt count for the footer caption. */
  promptCount: number;
}

function toneFor(rank: number): "ok" | "warn" | "danger" {
  if (rank <= 3) return "ok";
  if (rank <= 5) return "warn";
  return "danger";
}

export function ProviderRanks({
  providers,
  brand,
  promptCount,
}: ProviderRanksProps) {
  return (
    <div className="flex flex-col gap-[10px]">
      {providers.map((p) => {
        const tone = toneFor(p.rank);
        const identity = resolveProviderIdentity(p.provider);
        return (
          <div
            key={p.provider}
            className="grid grid-cols-[minmax(0,1.3fr)_auto_minmax(70px,1fr)_auto] items-center gap-[12px]"
          >
            <div className="flex min-w-0 items-center gap-[8px]">
              <ProviderDot provider={p.provider} />
              <span
                title={identity.label}
                className="truncate text-[length:var(--font-size-sm)]"
              >
                {identity.label}
              </span>
            </div>
            <div className="whitespace-nowrap font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
              rank {p.rank.toFixed(2)}
            </div>
            <Bar
              value={Math.max(0, 10 - p.rank)}
              max={10}
              color={`var(--${tone})`}
              height={6}
              ariaLabel={`${identity.label} rank score`}
            />
            <div className="whitespace-nowrap text-right font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
              {(p.rate * 100).toFixed(0)}% seen · {p.count}r
            </div>
          </div>
        );
      })}
      <div className="mt-[6px] flex items-center gap-[6px] border-t border-[color:var(--hairline)] pt-[8px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        brand={brand.toLowerCase()} · window=7d · prompts={promptCount} · runs=
        {providers.reduce((a, p) => a + p.count, 0)}
      </div>
    </div>
  );
}
